pub mod settings;

use orion::kdf::{Password, Salt, derive_key};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use sea_orm_migration::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

use self::settings::DatabaseSettings;
use crate::client::adapters::out::persistance::migrations::Migrator;
use crate::client::core::initializers::settings::Settings;

const KDF_ITERATIONS: u32 = 3;
const KDF_MEMORY_KIB: u32 = 1 << 16;
const KDF_KEY_LEN: u32 = 32;

fn derive_sqlcipher_key(pin: &str, salt: &str) -> Result<Zeroizing<String>, sea_orm::DbErr> {
    let constant_salt = Salt::from_slice(salt.as_bytes())
        .map_err(|_| sea_orm::DbErr::Custom("Invalid salt configuration".to_string()))?;
    let password = Password::from_slice(pin.as_bytes())
        .map_err(|_| sea_orm::DbErr::Custom("Invalid password format".to_string()))?;

    let derived_key = derive_key(
        &password,
        &constant_salt,
        KDF_ITERATIONS,
        KDF_MEMORY_KIB,
        KDF_KEY_LEN,
    )
    .map_err(|_| sea_orm::DbErr::Custom("Key derivation failed".to_string()))?;

    let mut hex_key = Zeroizing::new(String::with_capacity((KDF_KEY_LEN * 2) as usize));
    for b in derived_key.unprotected_as_bytes() {
        let _ = write!(&mut *hex_key, "{:02x}", b);
    }

    Ok(Zeroizing::new(format!("\"x'{}'\"", hex_key.as_str())))
}

pub struct DatabaseProvider {
    connection: Arc<DatabaseConnection>,
    salt: String,
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

        let sqlcipher_raw_key = derive_sqlcipher_key(db_key, salt)?;

        let mut options = ConnectOptions::new(db_url);
        options
            .sqlx_logging(false)
            .max_connections(5)
            .min_connections(1)
            .connect_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(10 * 60))
            .sqlcipher_key(sqlcipher_raw_key.as_str().to_owned());

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

    /// Re-encrypts the already-open database in place under a new PIN, using the
    /// same salt the provider was constructed with. On success the database file on
    /// disk is rewritten to use the new key immediately — but the *caller* is
    /// responsible for persisting the new PIN wherever `Settings` reads it from
    /// (env var / config file), otherwise the next `DatabaseProvider::new` will
    /// derive the old key and fail to open the file.
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
