pub mod settings;

use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use sea_orm_migration::prelude::*;
use std::sync::Arc;
use std::time::Duration;

use self::settings::DatabaseSettings;
use crate::core::initializers::settings::Settings;
use migration::Migrator;
use migration::connection::{connect_options_with_key, derive_sqlcipher_key};

pub struct DatabaseProvider {
    connection: Arc<DatabaseConnection>,
    salt: String,
}

impl DatabaseProvider {
    pub async fn new(settings: Arc<Settings>, sqlcipher_key: &str) -> Result<Self, sea_orm::DbErr> {
        let db_url = &settings.database.url;

        let Some(salt) = &settings.database.salt else {
            tracing::error!("No encryption salt passed");
            return Err(sea_orm::DbErr::Custom("No salt passed".to_string()));
        };

        let mut options = connect_options_with_key(db_url, sqlcipher_key);
        options
            .max_connections(5)
            .min_connections(1)
            .connect_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(10 * 60));

        let connection = Database::connect(options).await?;

        connection
            .execute_unprepared("SELECT count(*) FROM sqlite_master;")
            .await
            .map_err(|e| {
                tracing::error!("Database key verification failed: {e}");
                sea_orm::DbErr::Custom(
                    "Database key verification failed (wrong key or corrupted file)".to_string(),
                )
            })?;

        Ok(Self {
            connection: Arc::new(connection),
            salt: salt.clone(),
        })
    }

    pub fn connection(&self) -> Arc<DatabaseConnection> {
        self.connection.clone()
    }

    pub async fn init_db(&self) -> Result<(), sea_orm::DbErr> {
        Migrator::up(self.connection.as_ref(), None).await?;

        Ok(())
    }

    pub async fn rekey(&self, new_pin: &str) -> Result<(), sea_orm::DbErr> {
        DatabaseSettings::validate_pin(new_pin).map_err(sea_orm::DbErr::Custom)?;

        let new_raw_key = derive_sqlcipher_key(new_pin, &self.salt)?;

        self.connection
            .execute_unprepared(&format!("PRAGMA rekey = {};", new_raw_key.as_str()))
            .await
            .map_err(|e| {
                tracing::error!("Database rekey failed: {e}");
                sea_orm::DbErr::Custom("Database rekey failed".to_string())
            })?;

        tracing::info!("Database successfully re-encrypted with new key");

        Ok(())
    }
}
