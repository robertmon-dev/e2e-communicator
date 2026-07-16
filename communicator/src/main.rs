pub mod client;
pub mod crypto;

use crate::client::core::initializers::database::DatabaseProvider;
use crate::client::core::initializers::logger;
use crate::client::core::initializers::settings::Settings;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let settings = Arc::new(Settings::new().expect("Failed to load .env variables"));

    logger::init(&settings);
    tracing::info!("Booting application...");

    let database_provider = match DatabaseProvider::new(settings).await {
        Ok(provider) => Arc::new(provider),
        Err(e) => {
            tracing::error!("Failed to initialize database connection: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = database_provider.init_db().await {
        tracing::error!("Database migration failed during startup: {e}");
        std::process::exit(1);
    }

    tracing::info!("Database initialized and migrated successfully!");
}
