use std::collections::HashMap;
use std::process::exit;

use clap::StructOpt;
use kstring::KString;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug)]
enum OutputType {
    Json,
    Text,
    Ignore,
}

#[derive(Serialize, Deserialize, Debug)]
enum Prehook {
    Exec {
        bin: String,
        id: String,
        args: Vec<String>,
        stdout: OutputType,
        stderr: OutputType,
    },
    Render {
        path: String,
    },
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
            prehooks: vec![
                Prehook::Exec {
                    id: "hoge".to_owned(),
                    bin: "/usr/local/bin/hoge".to_owned(),
                    args: vec![],
                    stderr: OutputType::Ignore,
                    stdout: OutputType::Json,
                },
                Prehook::Render {
                    path: "/etc/envoy/envoy.yml".to_owned(),
                },
            ],
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
    liquid::object!({
        "exec": liquid::Object::new(),
    })
}

fn update_env(globals: &mut liquid::Object) {
    globals.insert(
        KString::from_static("env"),
        liquid::model::Value::Object(create_env_object()),
    );
}

fn json_to_liquid_value(json: &json::JsonValue) -> liquid::model::Value {
    use json::JsonValue;
    match json {
        JsonValue::Array(arr) => liquid::model::Value::array(arr.iter().map(json_to_liquid_value)),
        JsonValue::Null => liquid::model::Value::Nil,
        JsonValue::Short(short) => liquid::model::Value::scalar(short.as_str().to_owned()),
        JsonValue::Number(num) => liquid::model::Value::scalar(num.to_string()),
        JsonValue::String(s) => liquid::model::Value::scalar(s.to_owned()),
        JsonValue::Boolean(b) => liquid::model::Value::scalar(b.to_string()),
        JsonValue::Object(json) => {
            let mut obj = liquid::Object::new();
            json.iter().for_each(|(k, v)| {
                obj.insert(KString::from_string(k.to_owned()), json_to_liquid_value(v));
            });
            liquid::model::Value::Object(obj)
        }
    }
}

fn create_output_object(
    out: &str,
    ty: &OutputType,
) -> Result<Option<liquid::model::Value>, json::Error> {
    match ty {
        OutputType::Ignore => Ok(None),
        OutputType::Json => {
            let json = json::parse(out)?;
            Ok(Some(json_to_liquid_value(&json)))
        }
        OutputType::Text => Ok(Some(liquid::model::Value::scalar(out.to_owned()))),
    }
}

fn register_output_value(
    global: &mut liquid::Object,
    id: &str,
    stdout: Option<liquid::model::Value>,
    stderr: Option<liquid::model::Value>,
) {
    let out = liquid::object!({
        "stderr": stderr,
        "stdout": stdout,
    });
    global
        .get_mut("exec")
        .expect("must exist")
        .as_object_mut()
        .expect("must be object")
        .insert(
            KString::from_string(id.to_owned()),
            liquid::model::Value::Object(out),
        );
}

fn execute(config: &Config) -> Result<(), String> {
    use std::io::Write;
    use std::process;
    let mut globals = init_globals();
    for prehook in &config.prehooks {
        match prehook {
            Prehook::Exec {
                id,
                bin,
                args,
                stdout,
                stderr,
            } => {
                let out = process::Command::new(bin)
                    .args(args)
                    .output()
                    .map_err(|e| format!("Cannot execute prehook {} due to {:?}", bin, e))?;
                let stdout = create_output_object(&String::from_utf8_lossy(&out.stdout), stdout)
                    .map_err(|e| {
                        format!("Cannot parse stdout of {} as json due to {:?}", bin, e)
                    })?;
                let stderr = create_output_object(&String::from_utf8_lossy(&out.stderr), stderr)
                    .map_err(|e| {
                        format!("Cannot parse stderr of {} as json due to {:?}", bin, e)
                    })?;
                register_output_value(&mut globals, id, stdout, stderr);
            }
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
