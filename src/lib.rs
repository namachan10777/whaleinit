use nix::{
    errno::Errno,
    sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal},
    unistd::Pid,
};
use serde::Deserialize;
use std::{
    io::{BufRead as _, Read},
    path::{Path, PathBuf},
    process::Stdio,
};
use tracing::{error, info, trace, warn};
use valuable::Valuable;

#[derive(Deserialize, Valuable)]
struct ServiceConfig {
    title: String,
    exec: String,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to read service directory: {0}")]
    ReadServiceDir(std::io::Error),
    #[error("Failed to read service file: {0}: {1}")]
    ReadServiceFile(PathBuf, std::io::Error),
    #[error("Failed to parse service file: {0}: {1}")]
    ParseServiceFile(PathBuf, toml::de::Error),
    #[error("Failed to launch service: {service}: {error}")]
    LaunchService {
        service: String,
        error: std::io::Error,
    },
    #[error("Failed to set signal handler: {signal} {errno}")]
    SetSigAction { errno: Errno, signal: Signal },
}

fn read_services<P: AsRef<Path>>(service_dir: P) -> Result<Vec<ServiceConfig>, Error> {
    let mut services = Vec::new();
    for file in std::fs::read_dir(service_dir).map_err(Error::ReadServiceDir)? {
        let entry = file.map_err(Error::ReadServiceDir)?;
        if !entry
            .file_type()
            .map_err(|e| Error::ReadServiceFile(entry.path(), e))?
            .is_file()
        {
            trace!(path=?entry.path(), "Skipping non-file entry");
            continue;
        }
        let service = std::fs::read_to_string(entry.path())
            .map_err(|e| Error::ReadServiceFile(entry.path(), e))?;
        let service: ServiceConfig =
            toml::from_str(&service).map_err(|e| Error::ParseServiceFile(entry.path(), e))?;
        trace!(service=service.as_value(), path=?entry.path(), "service is defined");
        services.push(service);
    }
    Ok(services)
}

fn print_log<R: Read>(out: R, title: &str, log_type: &str) {
    let reader = std::io::BufReader::new(out);
    for line in reader.lines() {
        match line {
            Ok(line) => {
                info!(service = title, line = line, type=log_type, "log");
            }
            Err(e) => {
                error!(
                    service = title,
                    error = e.to_string(),
                    type = log_type,
                    "failed to read"
                );
            }
        }
    }
}

fn handle(service: &ServiceConfig) -> Result<(), Error> {
    let mut command = std::process::Command::new(&service.exec);
    command.args(&service.args);

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::LaunchService {
            service: service.title.clone(),
            error: e,
        })?;

    info!(pid = child.id(), service = service.title, "service started");

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    std::thread::scope(|scope| {
        let waiter = scope.spawn(move || {
            let _ = child.wait();
        });

        scope.spawn(move || {
            print_log(stdout, &service.title, "stdout");
        });

        scope.spawn(move || {
            print_log(stderr, &service.title, "stderr");
        });

        waiter.join().unwrap();
    });

    Ok(())
}

extern "C" fn handle_propagational_signal(signal: i32) {
    let Ok(signal) = nix::sys::signal::Signal::try_from(signal) else {
        warn!(signal, "invalid signal");
        return;
    };
    if let Err(e) = nix::sys::signal::kill(Pid::from_raw(-1), signal) {
        error!(
            error = e.to_string(),
            signal = signal.as_str(),
            "failed to send signal"
        );
    } else {
        info!(signal = signal.as_str(), "signal sent");
    }
}

fn set_propagational_signal_sigactions<I: IntoIterator<Item = Signal>>(
    propagational_signals: I,
) -> Result<(), Error> {
    for propagational_signal in propagational_signals {
        let handler = SigHandler::Handler(handle_propagational_signal);
        let sigaction = SigAction::new(handler, SaFlags::empty(), SigSet::empty());
        unsafe {
            nix::sys::signal::sigaction(propagational_signal, &sigaction).map_err(|e| {
                Error::SetSigAction {
                    errno: e,
                    signal: propagational_signal,
                }
            })?;
        }
    }
    Ok(())
}

fn set_sigactions() -> Result<(), Error> {
    set_propagational_signal_sigactions([Signal::SIGINT, Signal::SIGTERM])?;
    Ok(())
}

pub fn run<P: AsRef<Path>>(service_dir: P) -> Result<(), Error> {
    let services = read_services(service_dir)?;

    let mut wait_handlers = Vec::new();

    for service in services {
        wait_handlers.push(std::thread::spawn(move || {
            if let Err(e) = handle(&service) {
                error!(
                    error = e.to_string(),
                    service = service.title,
                    "failed to handle service"
                );
            }
        }));
    }

    set_sigactions()?;

    std::thread::spawn(move || {
        loop {
            let Ok(status) = nix::sys::wait::wait().inspect_err(|e| {
                warn!(error = e.to_string(), "failed to wait child process");
            }) else {
                continue;
            };
            match status {
                nix::sys::wait::WaitStatus::Exited(pid, code) => {
                    info!(pid = pid.as_raw(), code, "child process exited");
                }
                nix::sys::wait::WaitStatus::Signaled(pid, signal, _) => {
                    info!(
                        pid = pid.as_raw(),
                        signal = signal.as_str(),
                        "child process signaled"
                    );
                }
                nix::sys::wait::WaitStatus::Stopped(pid, signal) => {
                    info!(
                        pid = pid.as_raw(),
                        signal = signal.as_str(),
                        "child process stopped"
                    );
                }
                nix::sys::wait::WaitStatus::Continued(pid) => {
                    info!(pid = pid.as_raw(), "child process continued");
                }
                nix::sys::wait::WaitStatus::StillAlive => {}
                #[cfg(target_os = "linux")]
                nix::sys::wait::WaitStatus::PtraceEvent(pid, signal, event) => {
                    info!(
                        pid = pid.as_raw(),
                        signal = signal.as_str(),
                        event,
                        "ptrace event"
                    );
                }
                #[cfg(target_os = "linux")]
                nix::sys::wait::WaitStatus::PtraceSyscall(pid) => {
                    info!(pid = pid.as_raw(), "ptrace syscall");
                }
            }
        }
    });

    for wait_handler in wait_handlers {
        wait_handler.join().unwrap();
    }

    Ok(())
}
