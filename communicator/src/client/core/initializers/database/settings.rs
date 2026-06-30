use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub url: String,
    pub encryption_key: Option<String>,
    pub encryption_salt: Option<String>,
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            url: "sqlite://data.db?mode=rwc".to_string(),
            encryption_key: None,
            encryption_salt: None,
        }
    }
}
