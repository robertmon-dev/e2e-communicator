use std::env;

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

use crate::core::initializers::database::settings::DatabaseSettings;
use crate::core::initializers::logger::settings::LoggingSettings;

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AppEnv {
    Local,
    Test,
    Production,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RelaySettings {
    pub endpoint: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub environment: AppEnv,
    pub logging: LoggingSettings,
    pub database: DatabaseSettings,
    pub relay: RelaySettings,
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

        let settings: Settings = s.try_deserialize()?;

        if let Some(key) = &settings.database.encryption_key {
            DatabaseSettings::validate_pin(key).map_err(ConfigError::Message)?;
        }

        Ok(settings)
    }
}
