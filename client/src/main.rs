use std::sync::Arc;

use client::adapters::r#in::ipc::facade::Facade;
use client::adapters::r#in::ipc::server as ipc_server;
use client::adapters::r#in::web::grpc::identity::GrpcIdentityClient;
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
use client::domain::services::authenticator::Authenticator;
use client::domain::services::connectivity_monitor::ConnectivityMonitor;
use client::domain::services::encryptor::Encryptor;
use client::domain::services::exchanger::Exchanger;
use client::domain::services::inbox::Inbox;
use client::domain::services::messenger::Messenger;
use client::domain::services::relay::Relay;
use client::domain::state::connectivity::ConnectivityState;
use client::domain::state::facade_events::{
    bridge_connectivity, bridge_identity, facade_event_bus,
};
use client::domain::state::identity::{IdentityState, IdentityStatus};
use client::domain::state::presence::PresenceState;
use client::domain::state::session::SessionState;
use migration::connection::derive_boot_keys;
use tokio::sync::watch;
use tonic::transport::Channel;

#[tokio::main]
async fn main() {
    let (shutdown_tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();

        #[cfg(unix)]
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => signal.recv().await,
                Err(e) => {
                    tracing::warn!("Failed to register hanlder for SIGTERM: {e}");
                    std::future::pending::<Option<()>>().await
                }
            }
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            res = ctrl_c => {
                if let Err(e) = res {
                    tracing::error!("Error while listening for SIGINT: {e}");
                } else {
                    tracing::info!("Received signal for SIGINT. Shutting down.");
                }
            }
            _ = terminate => {
                tracing::info!("Received signal for SIGTERM. Shutting down.");
            }
        }

        let _ = shutdown_tx.send(true);
    });

    let settings = Arc::new(Settings::new().expect("Failed to load .env variables"));

    logger::init(&settings);
    tracing::info!("Booting application...");

    let boot_keys = match (&settings.database.encryption_key, &settings.database.salt) {
        (Some(db_key), Some(db_salt)) => match derive_boot_keys(db_key, db_salt) {
            Ok(keys) => keys,
            Err(e) => {
                tracing::error!("Failed to derive application keys: {e}");
                std::process::exit(1);
            }
        },
        _ => {
            tracing::error!("Missing database encryption key or salt");
            std::process::exit(1);
        }
    };

    let database_provider =
        match DatabaseProvider::new(settings.clone(), boot_keys.sqlcipher_key.as_str()).await {
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
    let master_key = boot_keys.master_key;
    let exchanger_master_key = boot_keys.exchanger_master_key;

    let user_repository = Arc::new(UserRepository::new(db.clone()));
    let attachment_repository = Arc::new(AttachmentRepository::new(db.clone()));
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
    let local_identity_repository = Arc::new(LocalIdentityRepository::new(
        db.clone(),
        device_key_store.clone(),
    ));

    let identity_status = match local_identity_repository.get().await {
        Ok(Some(identity)) => {
            tracing::info!(
                user_id = %identity.user_id,
                device_id = %identity.device_id,
                "Local identity loaded"
            );
            IdentityStatus::Registered(identity)
        }
        Ok(None) => {
            tracing::warn!(
                "No local identity: starting unregistered. Register or link a device through the IPC facade."
            );
            IdentityStatus::Unregistered
        }
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

    let session_state = Arc::new(SessionState::default());
    let identity_state = Arc::new(IdentityState::new(identity_status));

    let connectivity_monitor = ConnectivityMonitor::new(connectivity_state.clone());

    let rx_clone = rx.clone();
    let connectivity_monitor_handle = tokio::spawn(async move {
        connectivity_monitor.run(rx_clone).await;
    });

    tracing::info!("Connectivity monitor started");

    let channel = match Channel::from_shared(settings.relay.endpoint.clone()) {
        Ok(endpoint) => match endpoint.connect().await {
            Ok(channel) => channel,
            Err(e) => {
                tracing::error!("Failed to connect to relay server: {e}");
                std::process::exit(1);
            }
        },
        Err(e) => {
            tracing::error!("Invalid relay endpoint '{}': {e}", settings.relay.endpoint);
            std::process::exit(1);
        }
    };

    let relay_client = Arc::new(GrpcRelayClient::with_channel(
        channel.clone(),
        session_state.clone(),
        identity_state.clone(),
    ));
    let identity_client = Arc::new(GrpcIdentityClient::with_channel(
        channel,
        session_state.clone(),
    ));

    let authenticator = Arc::new(Authenticator::new(
        identity_client,
        device_key_store,
        local_identity_repository,
        identity_state.clone(),
        session_state.clone(),
        connectivity_state.clone(),
    ));
    let rx_clone = rx.clone();
    let authenticator_handle = {
        let authenticator = authenticator.clone();
        tokio::spawn(async move {
            authenticator.run(rx_clone).await;
        })
    };

    tracing::info!("Authenticator started");

    let relay = Relay::new(
        outbox_repository.clone(),
        connectivity_state.clone(),
        session_state.clone(),
        relay_client.clone(),
    );
    let rx_copy = rx.clone();
    let relay_handle = tokio::spawn(async move {
        relay.run(rx_copy).await;
    });

    tracing::info!("Relay started");

    let inbox = Inbox::new(
        relay_client,
        message_repository,
        outbox_repository,
        exchanger,
        connectivity_state.clone(),
        identity_state.clone(),
        session_state.clone(),
        facade_events.clone(),
    );
    let rx_copy = rx.clone();
    let inbox_handle = tokio::spawn(async move {
        inbox.run(rx_copy).await;
    });

    tracing::info!("Inbox started");

    let connectivity_bridge_handle = tokio::spawn(bridge_connectivity(
        connectivity_state.clone(),
        facade_events.clone(),
        rx.clone(),
    ));
    let identity_bridge_handle = tokio::spawn(bridge_identity(
        identity_state.clone(),
        session_state.clone(),
        facade_events.clone(),
        rx.clone(),
    ));

    let facade = Arc::new(Facade::new(
        messenger,
        chat_repository,
        user_repository,
        Arc::new(MessageRepository::new(db.clone())),
        attachment_repository,
        connectivity_state.clone(),
        identity_state,
        session_state,
        authenticator,
        settings.facade.session_token.clone(),
    ));

    let facede_events_clone = facade_events;
    let socket_path = settings
        .facade
        .socket_path
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(ipc_server::default_socket_path);

    let rx_clone = rx;
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = ipc_server::run(&socket_path, facade, facede_events_clone, rx_clone).await {
            tracing::error!("IPC server failed: {e}");
        }
    });

    let _ = tokio::join!(
        connectivity_monitor_handle,
        authenticator_handle,
        relay_handle,
        inbox_handle,
        connectivity_bridge_handle,
        identity_bridge_handle,
        ipc_handle
    );
}
