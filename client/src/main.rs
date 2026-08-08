pub mod adapters;
pub mod core;
pub mod crypto;
pub mod domain;

use std::sync::Arc;

use crate::adapters::r#in::web::grpc::transport::GrpcRelayClient;
use crate::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use crate::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::adapters::out::persistance::repositories::local_identity::LocalIdentityRepository;
use crate::adapters::out::persistance::repositories::message::MessageRepository;
use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::status_schedule::StatusScheduleRepository;
use crate::adapters::out::persistance::repositories::user::UserRepository;
use crate::core::initializers::database::DatabaseProvider;
use crate::core::initializers::logger;
use crate::core::initializers::settings::Settings;
use crate::domain::services::connectivity_monitor::ConnectivityMonitor;
use crate::domain::services::relay::Relay;
use crate::domain::state::connectivity::ConnectivityState;
use crate::domain::state::presence::PresenceState;

#[tokio::main]
async fn main() {
    let settings = Arc::new(Settings::new().expect("Failed to load .env variables"));

    logger::init(&settings);
    tracing::info!("Booting application...");

    let database_provider = match DatabaseProvider::new(settings.clone()).await {
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

    let db = database_provider.connection();

    let _user_repository = Arc::new(UserRepository::new(db.clone()));
    let _attachment_repository = Arc::new(AttachmentRepository::new(db.clone()));
    let _key_repository = Arc::new(KeyRepository::new(db.clone()));
    let _message_repository = Arc::new(MessageRepository::new(db.clone()));
    let _chat_repository = Arc::new(ChatRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));
    let _status_schedule_repository = Arc::new(StatusScheduleRepository::new(db.clone()));
    let local_identity_repository = Arc::new(LocalIdentityRepository::new(db.clone()));

    let local_identity = match local_identity_repository.get_or_create().await {
        Ok(identity) => identity,
        Err(e) => {
            tracing::error!("Failed to load local identity: {e}");
            std::process::exit(1);
        }
    };

    let connectivity_state = Arc::new(ConnectivityState::default());
    let _presence_state = Arc::new(PresenceState::default());

    let connectivity_monitor = ConnectivityMonitor::new(connectivity_state.clone());
    let connectivity_monitor_handle = tokio::spawn(async move {
        connectivity_monitor.run().await;
    });

    tracing::info!("Connectivity monitor started");

    let relay_client = match GrpcRelayClient::connect(settings.relay.endpoint.clone()).await {
        Ok(client) => Arc::new(client),
        Err(e) => {
            tracing::error!("Failed to connect to relay server: {e}");
            std::process::exit(1);
        }
    };

    tracing::info!(
        user_id = %local_identity.user_id,
        device_id = %local_identity.device_id,
        "Local identity loaded"
    );

    let relay = Relay::new(outbox_repository, connectivity_state.clone(), relay_client);
    let relay_handle = tokio::spawn(async move {
        relay.run().await;
    });

    tracing::info!("Relay started");

    let _ = tokio::join!(connectivity_monitor_handle, relay_handle);
}
