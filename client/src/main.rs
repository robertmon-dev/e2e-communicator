use std::sync::Arc;

use client::adapters::r#in::ipc::facade::Facade;
use client::adapters::r#in::ipc::server as ipc_server;
use client::adapters::r#in::web::grpc::transport::GrpcRelayClient;
use client::adapters::out::keys::DeviceKeyStore;
use client::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use client::adapters::out::persistance::repositories::chat::ChatRepository;
use client::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use client::adapters::out::persistance::repositories::key::KeyRepository;
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
use client::domain::services::exchanger::Exchanger;
use client::domain::services::inbox::Inbox;
use client::domain::services::messenger::Messenger;
use client::domain::services::relay::Relay;
use client::domain::state::connectivity::ConnectivityState;
use client::domain::state::facade_events::{bridge_connectivity, facade_event_bus};
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

    let (master_key, exchanger_master_key) =
        match (&settings.database.encryption_key, &settings.database.salt) {
            (Some(db_key), Some(db_salt)) => {
                let first = derive_master_key(db_key, db_salt);
                let second = derive_master_key(db_key, db_salt);
                match (first, second) {
                    (Ok(a), Ok(b)) => (a, b),
                    _ => {
                        tracing::error!("Failed to derive application master key");
                        std::process::exit(1);
                    }
                }
            }
            _ => {
                tracing::error!("Missing database encryption key or salt");
                std::process::exit(1);
            }
        };

    let _user_repository = Arc::new(UserRepository::new(db.clone()));
    let _attachment_repository = Arc::new(AttachmentRepository::new(db.clone()));
    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let chat_repository = Arc::new(ChatRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));
    let _status_schedule_repository = Arc::new(StatusScheduleRepository::new(db.clone()));
    let key_repository = Arc::new(KeyRepository::new(db.clone()));
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

    let exchanger = Arc::new(Exchanger::new(key_repository, exchanger_master_key));

    let messenger = Arc::new(Messenger::new(
        message_repository.clone(),
        chat_repository.clone(),
        outbox_repository.clone(),
        exchanger.clone(),
    ));

    let facade_events = facade_event_bus();

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

    let relay = Relay::new(
        outbox_repository.clone(),
        connectivity_state.clone(),
        relay_client.clone(),
    );
    let relay_handle = tokio::spawn(async move {
        relay.run().await;
    });

    tracing::info!("Relay started");

    let inbox = Inbox::new(
        relay_client,
        message_repository,
        outbox_repository,
        exchanger,
        connectivity_state.clone(),
        local_identity.user_id,
        local_identity.device_id,
        facade_events.clone(),
    );
    let inbox_handle = tokio::spawn(async move {
        inbox.run().await;
    });

    tracing::info!("Inbox started");

    let connectivity_bridge_handle = tokio::spawn(bridge_connectivity(
        connectivity_state.clone(),
        facade_events.clone(),
    ));

    let facade = Arc::new(Facade::new(
        messenger,
        chat_repository,
        Arc::new(MessageRepository::new(db.clone())),
        connectivity_state.clone(),
        local_identity.user_id,
        local_identity.device_id,
    ));
    let socket_path = settings
        .facade
        .socket_path
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(ipc_server::default_socket_path);
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = ipc_server::run(&socket_path, facade, facade_events).await {
            tracing::error!("IPC server failed: {e}");
        }
    });

    tracing::info!("IPC facade server started");

    let _ = tokio::join!(
        connectivity_monitor_handle,
        relay_handle,
        inbox_handle,
        connectivity_bridge_handle,
        ipc_handle
    );
}
