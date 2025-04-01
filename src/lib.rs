use liquid::model::KString;
use nix::{
    errno::Errno,
    sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal},
    unistd::Pid,
};
use serde::Deserialize;
use std::{
    io::{BufRead as _, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt as _, PermissionsExt as _},
    path::PathBuf,
    process::Stdio,
};
use tracing::{error, info, trace, warn};
use valuable::Valuable;

#[derive(Deserialize, Valuable)]
pub struct ServiceConfig {
    pub title: String,
    pub exec: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub essential: bool,
}

#[derive(Deserialize, Valuable)]
pub struct Template {
    pub src: String,
    pub dest: String,
}

#[derive(Deserialize, Valuable)]
pub struct Config {
    pub services: Vec<ServiceConfig>,
    #[serde(default)]
    pub templates: Vec<Template>,
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
    #[error("Failed to read template source: {src}: {error}")]
    ReadTemplateSource { src: String, error: std::io::Error },
    #[error("Failed to render template: {src}: {error}")]
    RenderTemplate { src: String, error: liquid::Error },
    #[error("Failed to write template: {dest}: {error}")]
    WriteTemplate { dest: String, error: std::io::Error },
    #[error("Failed to change template ownership: {dest}: {error}")]
    ChangeTemplateOwnership { dest: String, error: std::io::Error },
}

fn trigger_shutdown(initial: Signal) {
    let signal_step = [Signal::SIGTERM, Signal::SIGINT, Signal::SIGKILL];
    let mut step = signal_step
        .iter()
        .position(|s| *s == initial)
        .unwrap_or_else(|| {
            info!(signal = initial.as_str(), "Send signal to all processes");
            if let Err(e) = nix::sys::signal::kill(Pid::from_raw(-1), initial) {
                error!(
                    error = e.to_string(),
                    signal = initial.as_str(),
                    "failed to send signal"
                );
                std::thread::sleep(std::time::Duration::from_secs(3));
                0
            } else {
                0
            }
        });

    while let Some(signal) = signal_step.get(step) {
        info!(signal = signal.as_str(), "Send signal to all processes");
        if let Err(e) = nix::sys::signal::kill(Pid::from_raw(-1), *signal) {
            error!(
                error = e.to_string(),
                signal = signal.as_str(),
                "failed to send signal"
            );
            std::thread::sleep(std::time::Duration::from_secs(3));
            step += 1;
        } else {
            break;
        }
    }
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
        scope.spawn(move || {
            print_log(stdout, &service.title, "stdout");
        });

        scope.spawn(move || {
            print_log(stderr, &service.title, "stderr");
        });

        match child.wait() {
            Ok(code) => {
                info!(
                    code = code.code(),
                    service = service.title,
                    "service exited"
                );
            }
            Err(e) if e.raw_os_error() == Some(nix::libc::ECHILD) => {
                trace!("no child process");
            }
            Err(e) => {
                warn!(
                    e = e.to_string(),
                    service = service.title,
                    "failed to wait for child process"
                );
            }
        }
        if service.essential {
            info!("essential service exited");
            trigger_shutdown(Signal::SIGTERM);
        }
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

fn reap_children() -> Result<(), Error> {
    loop {
        let status = match nix::sys::wait::wait() {
            Ok(status) => status,
            Err(Errno::ECHILD) => {
                trace!("no child process");
                return Ok(());
            }
            Err(e) => {
                warn!(error = e.to_string(), "failed to wait child process");
                continue;
            }
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
}

pub struct TemplateContext {
    parser: liquid::Parser,
    ctx: liquid::Object,
}

impl TemplateContext {
    pub fn build() -> Self {
        let env = std::env::vars().map(|(name, var)| {
            let key = KString::from_string(name);
            let var = liquid::model::Value::scalar(var);
            (key, var)
        });
        let env = liquid::Object::from_iter(env);
        let ctx = liquid::object!({
            "env": env,
        });
        Self {
            parser: liquid::ParserBuilder::with_stdlib()
                .build()
                .expect("failed to build liquid parser"),
            ctx,
        }
    }

    pub fn render(&self, src: &str) -> Result<String, liquid::Error> {
        let template = self.parser.parse(src)?;
        let rendered = template.render(&self.ctx)?;
        Ok(rendered)
    }

    pub fn render_template(&self, template: &Template) -> Result<(), Error> {
        let src =
            std::fs::read_to_string(&template.src).map_err(|e| Error::ReadTemplateSource {
                src: template.src.clone(),
                error: e,
            })?;
        let content = self.render(&src).map_err(|e| Error::RenderTemplate {
            src: template.src.clone(),
            error: e,
        })?;
        let meta = std::fs::metadata(&template.src).map_err(|e| Error::ReadTemplateSource {
            src: template.src.clone(),
            error: e,
        })?;

        let mut outfile = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(meta.permissions().mode())
            .open(&template.dest)
            .map_err(|e| Error::WriteTemplate {
                dest: template.dest.clone(),
                error: e,
            })?;

        outfile
            .write_all(content.as_bytes())
            .map_err(|e| Error::WriteTemplate {
                dest: template.dest.clone(),
                error: e,
            })?;

        std::os::unix::fs::chown(&template.dest, Some(meta.uid()), Some(meta.gid())).map_err(
            |e| Error::ChangeTemplateOwnership {
                dest: template.dest.clone(),
                error: e,
            },
        )?;

        Ok(())
    }
}

pub fn run<I: IntoIterator<Item = ServiceConfig>>(services: I) -> Result<(), Error> {
    set_sigactions()?;

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

    std::thread::spawn(reap_children);

    for wait_handler in wait_handlers {
        wait_handler.join().unwrap();
    }

    Ok(())
}
