use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub url: String,
    pub encryption_key: Option<String>,
    pub salt: Option<String>,
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            url: "sqlite://data.db?mode=rwc".to_string(),
            encryption_key: None,
            salt: None,
        }
    }
}

impl DatabaseSettings {
    pub fn validate_pin(pin: &str) -> Result<(), String> {
        if pin.len() != 8 {
            return Err("Database encryption key must be exactly 8 characters long".to_string());
        }

        if !pin.chars().all(|c| c.is_ascii_digit()) {
            return Err("Database encryption key must contain only digits".to_string());
        }

        Ok(())
    }
}
