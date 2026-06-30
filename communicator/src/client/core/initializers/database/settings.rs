use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub url: String,
    pub encryption_key: Option<String>,
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            encryption_key: None,
            url: "sqlite://data.db?mode=rwc".to_string(),
        }
    }
}
