use clap::Parser;
use tracing::{error, level_filters::LevelFilter};

#[derive(Parser)]
struct Opts {
    #[clap(long, env)]
    log_json: bool,
    #[clap(long, env, default_value = "true")]
    log_color: bool,
    #[clap(long, env)]
    log_init_filter: Option<String>,
    #[clap(short, long, env, default_value = "/etc/whaleinit/services")]
    service_dir: String,
}

fn init_subscriber<S: AsRef<str>>(filter: Option<S>, color: bool, json: bool) {
    use tracing_subscriber::prelude::*;
    let env_filter = if let Some(filter) = filter {
        tracing_subscriber::EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .parse_lossy(filter)
    } else {
        tracing_subscriber::EnvFilter::from_default_env()
    };

    let printer = tracing_subscriber::fmt::layer().with_ansi(color);
    let printer = if json {
        printer.json().boxed()
    } else {
        printer.boxed()
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(printer)
        .init();
}

fn main() {
    let opts = Opts::parse();

    init_subscriber(opts.log_init_filter.as_ref(), opts.log_color, opts.log_json);
    if let Err(e) = whaleinit::run(&opts.service_dir) {
        error!(error = e.to_string(), "fatal error");
    }
}
