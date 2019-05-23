use log::info;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use toml;
use yansi::Paint;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub service: Service,
    pub metrics: Metrics,
    pub message_expiry_days: i64,
}

#[derive(Debug, Deserialize)]
pub struct Service {
    pub worker_threads: usize,
    pub ca_cert_path: String,
    pub tls_cert_path: String,
    pub tls_key_path: String,
    pub bind_to_address: String,
}

#[derive(Debug, Deserialize)]
pub struct Metrics {
    pub bind_to_address: String,
}

fn get_switchroom_toml_path() -> String {
    env::var("SWITCHROOM_TOML").unwrap_or_else(|_| "Switchroom.toml".to_string())
}

lazy_static! {
    pub static ref CONFIG: Config = {
        let switchroom_toml_path = get_switchroom_toml_path();
        let config: Config = toml::from_str(&read_file_to_string(&switchroom_toml_path)).unwrap();
        config
    };
}

fn read_file_to_string(filename: &str) -> String {
    let mut file = File::open(filename).expect("Unable to open the file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Unable to read the file");
    contents
}

pub fn load_config() {
    info!(
        "Loaded Switchroom configuration values from {}",
        get_switchroom_toml_path()
    );
    info!("CONFIG => {:#?}", Paint::red(&*CONFIG));
}
