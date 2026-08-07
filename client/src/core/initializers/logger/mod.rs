pub mod settings;

use self::settings::LoggingLevel;
use crate::core::initializers::settings::{AppEnv, Settings};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init(settings: &Settings) {
    let filter_level = match settings.logging.logging_level {
        LoggingLevel::Trace => "trace",
        LoggingLevel::Debug => "debug",
        LoggingLevel::Info => "info",
        LoggingLevel::Warn => "warn",
        LoggingLevel::Error => "error",
        LoggingLevel::None => "off",
    };

    let filter = EnvFilter::new(filter_level);
    let registry = tracing_subscriber::registry().with(filter);

    if settings.environment == AppEnv::Production {
        registry.with(fmt::layer().json()).init();
    } else {
        registry.with(fmt::layer().pretty()).init();
    }

    tracing::info!(level = %filter_level, "Logger initialized");
}
