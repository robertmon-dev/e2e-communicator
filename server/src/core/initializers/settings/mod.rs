use std::env;

use common::logging::LoggingSettings;
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

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PushSettings {
    pub service_account_path: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub environment: AppEnv,
    pub logging: LoggingSettings,
    pub server: ServerSettings,
    pub redis: RedisSettings,
    #[serde(default)]
    pub push: PushSettings,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();

        let env_str = env::var("APP_ENV").unwrap_or_else(|_| "local".into());

        let s = Config::builder()
            .add_source(File::with_name("config/default").required(false))
            .add_source(File::with_name(&format!("config/{}", env_str)).required(false))
            .add_source(
                Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true)
                    .list_separator(",")
                    .with_list_parse_key("redis.sentinel_nodes"),
            )
            .set_override("environment", env_str)?
            .build()?;

        s.try_deserialize()
    }
}
