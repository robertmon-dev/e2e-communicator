pub mod entities;
pub mod migrations;
pub mod settings;

use orion::kdf::{Password, Salt, derive_key};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::sync::Arc;
use std::time::Duration;

use crate::client::core::initializers::settings::Settings;

pub struct DatabaseProvider {
    connection: Arc<DatabaseConnection>,
}

impl DatabaseProvider {
    pub async fn new(settings: Arc<Settings>) -> Result<Self, sea_orm::DbErr> {
        let db_url = &settings.database.url;
        let Some(db_key) = &settings.database.encryption_key else {
            tracing::error!("No encryption key passed");
            return Err(sea_orm::DbErr::Custom(
                "No encryption key passed".to_string(),
            ));
        };

        let Some(salt) = &settings.database.salt else {
            tracing::error!("No encryption salt passed");
            return Err(sea_orm::DbErr::Custom("No salt passed".to_string()));
        };

        let constant_salt = Salt::from_slice(salt.as_bytes())
            .map_err(|_| sea_orm::DbErr::Custom("Invalid salt configuration".to_string()))?;

        let password = Password::from_slice(db_key.as_bytes())
            .map_err(|_| sea_orm::DbErr::Custom("Invalid password format".to_string()))?;

        let derived_key = derive_key(&password, &constant_salt, 3, 1 << 16, 32)
            .map_err(|_| sea_orm::DbErr::Custom("Hash generation failed".to_string()))?;

        let hex_key: String = derived_key
            .unprotected_as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let sqlcipher_raw_key = format!("\"x'{}'\"", hex_key);

        let mut options = ConnectOptions::new(db_url);

        options
            .sqlx_logging(false)
            .max_connections(5)
            .min_connections(1)
            .connect_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(8))
            .sqlcipher_key(sqlcipher_raw_key);

        let connection = Database::connect(options).await?;

        Ok(Self {
            connection: Arc::new(connection),
        })
    }

    pub fn connection(&self) -> Arc<DatabaseConnection> {
        self.connection.clone()
    }
}
