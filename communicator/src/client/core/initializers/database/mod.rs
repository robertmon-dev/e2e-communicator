pub mod entities;
pub mod migrations;
pub mod settings;

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
        let cloned_key = db_key.clone();
        let mut options = ConnectOptions::new(db_url);

        options
            .sqlx_logging(false)
            .max_connections(5)
            .min_connections(1)
            .connect_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(8))
            .sqlcipher_key(cloned_key);

        let connection = Database::connect(options).await?;

        Ok(Self {
            connection: Arc::new(connection),
        })
    }

    pub fn connection(&self) -> Arc<DatabaseConnection> {
        self.connection.clone()
    }
}
