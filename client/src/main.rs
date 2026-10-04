use std::sync::Arc;

use client::adapters::r#in::web::grpc::transport::GrpcRelayClient;
use client::adapters::out::keys::DeviceKeyStore;
use client::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use client::adapters::out::persistance::repositories::chat::ChatRepository;
use client::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use client::adapters::out::persistance::repositories::local_identity::LocalIdentityRepository;
use client::adapters::out::persistance::repositories::message::MessageRepository;
use client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use client::adapters::out::persistance::repositories::status_schedule::StatusScheduleRepository;
use client::adapters::out::persistance::repositories::user::UserRepository;
use client::core::initializers::database::DatabaseProvider;
use client::core::initializers::logger;
use client::core::initializers::settings::Settings;
use client::domain::services::connectivity_monitor::ConnectivityMonitor;
use client::domain::services::encryptor::Encryptor;
use client::domain::services::relay::Relay;
use client::domain::state::connectivity::ConnectivityState;
use client::domain::state::presence::PresenceState;
use migration::connection::derive_master_key;

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

    let master_key = match (&settings.database.encryption_key, &settings.database.salt) {
        (Some(db_key), Some(db_salt)) => match derive_master_key(db_key, db_salt) {
            Ok(key) => key,
            Err(e) => {
                tracing::error!("Failed to derive application master key: {e}");
                std::process::exit(1);
            }
        },
        _ => {
            tracing::error!("Missing database encryption key or salt");
            std::process::exit(1);
        }
    };

    let _user_repository = Arc::new(UserRepository::new(db.clone()));
    let _attachment_repository = Arc::new(AttachmentRepository::new(db.clone()));
    let _message_repository = Arc::new(MessageRepository::new(db.clone()));
    let _chat_repository = Arc::new(ChatRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));
    let _status_schedule_repository = Arc::new(StatusScheduleRepository::new(db.clone()));
    let device_key_repository = Arc::new(DeviceKeyRepository::new(db.clone()));
    let device_key_store = Arc::new(DeviceKeyStore::new(
        device_key_repository,
        Encryptor::new(master_key),
    ));
    let mut local_identity_repository = LocalIdentityRepository::new(db.clone(), device_key_store);

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
