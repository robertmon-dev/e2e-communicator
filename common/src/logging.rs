use serde::Deserialize;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LoggingLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    None,
}

impl LoggingLevel {
    fn as_filter_str(&self) -> &'static str {
        match self {
            LoggingLevel::Trace => "trace",
            LoggingLevel::Debug => "debug",
            LoggingLevel::Info => "info",
            LoggingLevel::Warn => "warn",
            LoggingLevel::Error => "error",
            LoggingLevel::None => "off",
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingSettings {
    pub logging_level: LoggingLevel,
}

pub fn init(level: &LoggingLevel, json: bool) {
    let filter_level = level.as_filter_str();

    let filter = if matches!(level, LoggingLevel::Trace) {
        EnvFilter::new(filter_level)
    } else {
        EnvFilter::new(format!(
            "{filter_level},h2=warn,hyper=warn,hyper_util=warn,tower=warn"
        ))
    };
    let registry = tracing_subscriber::registry().with(filter);

    if json {
        registry.with(fmt::layer().json()).init();
    } else {
        registry.with(fmt::layer().pretty()).init();
    }

    tracing::info!(level = %filter_level, "Logger initialized");
}
