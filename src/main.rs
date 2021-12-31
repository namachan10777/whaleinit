use std::collections::HashMap;
use std::process::exit;

use clap::StructOpt;
use kstring::KString;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug)]
enum Prehook {
    Exec { bin: String, args: Vec<String> },
    Render { path: String },
}

#[derive(Serialize, Deserialize, Debug)]
struct Service {
    bin: String,
    args: Vec<String>,
    envs: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug)]
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

static LIQUID_PARSER: once_cell::sync::Lazy<liquid::Parser> =
    once_cell::sync::Lazy::new(|| liquid::ParserBuilder::with_stdlib().build().unwrap());

fn create_env_object() -> liquid::Object {
    let mut env = liquid::Object::new();
    for (k, v) in std::env::vars() {
        env.insert(KString::from_string(k), liquid::model::Value::scalar(v));
    }
    env
}

fn init_globals() -> liquid::Object {
    liquid::Object::new()
}

fn update_env(globals: &mut liquid::Object) {
    globals.insert(
        KString::from_static("env"),
        liquid::model::Value::Object(create_env_object()),
    );
}

fn execute(config: &Config) -> Result<(), String> {
    use std::io::Write;
    let mut globals = init_globals();
    for prehook in &config.prehooks {
        match prehook {
            Prehook::Exec { bin: _, args: _ } => unimplemented!(),
            Prehook::Render { path } => {
                update_env(&mut globals);
                let file = fs::read_to_string(path)
                    .map_err(|e| format!("Cannot read template file {} due to {:?}", path, e))?;
                let template = LIQUID_PARSER
                    .parse(&file)
                    .map_err(|e| format!("Cannot parse template file {} due to {:?}", path, e))?;
                let rendered = template
                    .render(&globals)
                    .map_err(|e| format!("Failed to render template {} due to {:?}", path, e))?;
                let mut file = fs::File::create(path).map_err(|e| {
                    format!("Cannot open file {} as write-mode due to {:?}", path, e)
                })?;
                file.write_all(rendered.as_bytes())
                    .map_err(|e| format!("Cannot write file {} due to {:?}", path, e))?;
            }
        }
    }
    Ok(())
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
    execute(&config)
}

fn main() {
    let opts = Opts::parse();
    if opts.example {
        match opts.config_syntax.as_str() {
            "ron" => println!(
                "{}",
                ron::ser::to_string_pretty(&Config::example(), ron::ser::PrettyConfig::default())
                    .unwrap()
            ),
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
