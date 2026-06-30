use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LoggingLevel {
    Trace,
    Debug,
    Error,
    Warn,
    Info,
    None,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingSettings {
    pub logging_level: LoggingLevel,
}
