use std::collections::HashMap;
use std::process::exit;

use clap::StructOpt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize)]
enum Prehook {
    Exec { bin: String, args: Vec<String> },
    Render { path: String },
}

#[derive(Serialize, Deserialize)]
struct Service {
    bin: String,
    args: Vec<String>,
    envs: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct Config {
    prehooks: Vec<Prehook>,
    services: Vec<Service>,
    log_files: Vec<String>,
}

impl Config {
    fn example() -> Self {
        Self {
            prehooks: vec![Prehook::Render {
                path: "/etc/envoy/envoy.yml".to_owned(),
            }],
            services: vec![
                Service {
                    bin: "/usr/bin/prometheus-node-exporter".to_owned(),
                    args: vec![],
                    envs: HashMap::new(),
                },
                Service {
                    bin: "/usr/bin/envoy".to_owned(),
                    args: vec![
                        "--config.file".to_owned(),
                        "/etc/envoy/envoy.yml".to_owned(),
                    ],
                    envs: HashMap::new(),
                },
            ],
            log_files: vec!["/dev/stdout".to_owned()],
        }
    }
}

#[derive(clap::Parser)]
struct Opts {
    #[clap(long, conflicts_with = "config")]
    example: bool,
    #[clap(long, default_value="ron", possible_values=["ron", "yaml"])]
    config_syntax: String,
    #[clap(short, long, required_unless_present = "example")]
    config: Option<String>,
}

enum ConfigSyntax {
    Ron,
    Yaml,
}

fn run<P>(config: P, syntax: &ConfigSyntax) -> Result<(), String>
where
    P: AsRef<Path>,
{
    let config = fs::read_to_string(&config).map_err(|e| {
        format!(
            "Cannot read config file ({:?}) due to {}",
            config.as_ref(),
            e
        )
    })?;
    let config = match syntax {
        ConfigSyntax::Ron => ron::from_str(&config)
            .map_err(|e| format!("Cannot parse config file due to {:?}", e))?,
        ConfigSyntax::Yaml => serde_yaml::from_str(&config)
            .map_err(|e| format!("Cannot parse config file due to {:?}", e))?,
    };
    println!("{:?}", config);
    Ok(())
}
fn main() {
    let opts = Opts::parse();
    if opts.example {
        match opts.config_syntax.as_str() {
            "ron" => println!("{}", ron::ser::to_string_pretty(&Config::example(), ron::ser::PrettyConfig::default()).unwrap()),
            "yaml" => println!("{}", serde_yaml::to_string(&Config::example()).unwrap()),
            _ => unreachable!(),
        }
    } else if let Some(config) = opts.config {
        let result = match opts.config_syntax.as_str() {
            "ron" => run(&config, &ConfigSyntax::Ron),
            "yaml" => run(&config, &ConfigSyntax::Yaml),
            _ => unreachable!(),
        };
        if let Err(msg) = result {
            eprintln!("{}", msg);
            exit(-1);
        }
    }
}
