use std::env;

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AppEnv {
    Local,
    Test,
    Production,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerSettings {
    pub bind_addr: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisSettings {
    pub sentinel_nodes: Vec<String>,
    pub service_name: String,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub environment: AppEnv,
    pub server: ServerSettings,
    pub redis: RedisSettings,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();

        let env_str = env::var("APP_ENV").unwrap_or_else(|_| "local".into());

        let s = Config::builder()
            .add_source(File::with_name("config/default").required(false))
            .add_source(File::with_name(&format!("config/{}", env_str)).required(false))
            .add_source(Environment::with_prefix("APP").separator("__"))
            .set_override("environment", env_str)?
            .build()?;

        s.try_deserialize()
    }
}
