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

    let database_provider = Arc::new(
        DatabaseProvider::new(settings)
            .await
            .expect("Failed to initialize database coonnection"),
    );
}
