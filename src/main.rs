use std::path::PathBuf;

use clap::Parser;
use tracing::{error, info, level_filters::LevelFilter};
use whaleinit::Config;

#[derive(Parser)]
struct Opts {
    #[clap(long, env)]
    log_json: bool,
    #[clap(long, env, default_value = "true")]
    log_color: bool,
    #[clap(long, env)]
    log_init_filter: Option<String>,
    #[clap(short, long, env, default_value = "/etc/whaleinit.toml")]
    config: PathBuf,
}

fn init_subscriber<S: AsRef<str>>(filter: Option<S>, color: bool, json: bool) {
    use tracing_subscriber::prelude::*;
    let env_filter = if let Some(filter) = filter {
        tracing_subscriber::EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .parse_lossy(filter)
            .boxed()
    } else {
        LevelFilter::INFO.boxed()
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

    let Ok(config) = std::fs::read_to_string(&opts.config).inspect_err(|e| {
        error!(error=e.to_string(), config=?opts.config, "read config");
    }) else {
        std::process::exit(1);
    };

    let template_context = whaleinit::TemplateContext::build();
    let Ok(config) = template_context.render(&config).inspect_err(|e| {
        error!(error=e.to_string(), config=?opts.config, "render config");
    }) else {
        std::process::exit(1);
    };

    let Ok(config) = toml::from_str::<whaleinit::Config>(&config).inspect_err(|e| {
        error!(error=e.to_string(), config=?opts.config, "parse config");
    }) else {
        std::process::exit(1);
    };

    let Config {
        services,
        templates,
        prehooks,
    } = config;

    for template in templates {
        if let Err(e) = template_context.render_template(&template) {
            error!(
                error = e.to_string(),
                src = template.src,
                dest = template.dest,
                "render template"
            );
            std::process::exit(1);
        } else {
            info!(
                src = template.src,
                dest = template.dest,
                "template rendered"
            );
        }
    }

    for prehook in prehooks {
        if let Err(e) = prehook.run().inspect_err(|e| {
            error!(error = e.to_string(), "run prehook");
        }) {
            error!(
                error = e.to_string(),
                prehook = prehook.display_name(),
                "run prehook"
            );
        } else {
            info!(prehook = prehook.display_name(), "prehook run");
        }
    }

    if let Err(e) = whaleinit::run(services) {
        error!(error = e.to_string(), "fatal error");
    }
}
