use std::sync::Arc;

use migration::connection::BootKeys;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tonic::transport::Channel;

use crate::adapters::r#in::ipc::facade::Facade;
use crate::adapters::r#in::web::grpc::identity::GrpcIdentityClient;
use crate::adapters::r#in::web::grpc::transport::GrpcRelayClient;
use crate::adapters::out::keys::DeviceKeyStore;
use crate::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use crate::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::adapters::out::persistance::repositories::local_identity::LocalIdentityRepository;
use crate::adapters::out::persistance::repositories::message::MessageRepository;
use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::status_schedule::StatusScheduleRepository;
use crate::adapters::out::persistance::repositories::user::UserRepository;
use crate::core::initializers::database::DatabaseProvider;
use crate::core::initializers::settings::Settings;
use crate::domain::services::authenticator::Authenticator;
use crate::domain::services::connectivity_monitor::ConnectivityMonitor;
use crate::domain::services::encryptor::Encryptor;
use crate::domain::services::exchanger::Exchanger;
use crate::domain::services::inbox::Inbox;
use crate::domain::services::messenger::Messenger;
use crate::domain::services::relay::Relay;
use crate::domain::state::connectivity::ConnectivityState;
use crate::domain::state::facade_events::{FacadeEventBus, bridge_connectivity, bridge_identity};
use crate::domain::state::identity::{IdentityState, IdentityStatus};
use crate::domain::state::presence::PresenceState;
use crate::domain::state::session::SessionState;

pub struct Started {
    pub facade: Arc<Facade>,
    pub handles: Vec<JoinHandle<()>>,
}

#[derive(Debug)]
pub enum BootError {
    WrongKey,
    Failed(String),
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongKey => f.write_str(DatabaseProvider::KEY_VERIFICATION_FAILED),
            Self::Failed(reason) => f.write_str(reason),
        }
    }
}

pub async fn start(
    settings: Arc<Settings>,
    boot_keys: BootKeys,
    events: FacadeEventBus,
    shutdown: watch::Receiver<bool>,
) -> Result<Started, BootError> {
    let database_provider =
        DatabaseProvider::new(settings.clone(), boot_keys.sqlcipher_key.as_str())
            .await
            .map(Arc::new)
            .map_err(open_error)?;

    database_provider
        .init_db()
        .await
        .map_err(|e| BootError::Failed(format!("Database migration failed during startup: {e}")))?;

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
            return Err(BootError::Failed(format!(
                "Failed to load local identity: {e}"
            )));
        }
    };

    let exchanger = Arc::new(Exchanger::new(key_repository, exchanger_master_key));

    let messenger = Arc::new(Messenger::new(
        message_repository.clone(),
        chat_repository.clone(),
        outbox_repository.clone(),
        exchanger.clone(),
    ));

    let connectivity_state = Arc::new(ConnectivityState::default());
    let _presence_state = Arc::new(PresenceState::default());

    let session_state = Arc::new(SessionState::default());
    let identity_state = Arc::new(IdentityState::new(identity_status));

    let mut handles = Vec::new();

    let connectivity_monitor = ConnectivityMonitor::new(connectivity_state.clone());
    let shutdown_rx = shutdown.clone();
    handles.push(tokio::spawn(async move {
        connectivity_monitor.run(shutdown_rx).await;
    }));

    tracing::info!("Connectivity monitor started");

    let channel = Channel::from_shared(settings.relay.endpoint.clone())
        .map_err(|e| {
            BootError::Failed(format!(
                "Invalid relay endpoint '{}': {e}",
                settings.relay.endpoint
            ))
        })?
        .connect_lazy();

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
    let shutdown_rx = shutdown.clone();
    handles.push({
        let authenticator = authenticator.clone();
        tokio::spawn(async move {
            authenticator.run(shutdown_rx).await;
        })
    });

    tracing::info!("Authenticator started");

    let relay = Relay::new(
        outbox_repository.clone(),
        connectivity_state.clone(),
        session_state.clone(),
        relay_client.clone(),
    );
    let shutdown_rx = shutdown.clone();
    handles.push(tokio::spawn(async move {
        relay.run(shutdown_rx).await;
    }));

    tracing::info!("Relay started");

    let inbox = Inbox::new(
        relay_client,
        message_repository,
        outbox_repository,
        exchanger,
        connectivity_state.clone(),
        identity_state.clone(),
        session_state.clone(),
        events.clone(),
    );
    let shutdown_rx = shutdown.clone();
    handles.push(tokio::spawn(async move {
        inbox.run(shutdown_rx).await;
    }));

    tracing::info!("Inbox started");

    handles.push(tokio::spawn(bridge_connectivity(
        connectivity_state.clone(),
        events.clone(),
        shutdown.clone(),
    )));
    handles.push(tokio::spawn(bridge_identity(
        identity_state.clone(),
        session_state.clone(),
        events,
        shutdown,
    )));

    let facade = Arc::new(Facade::new(
        messenger,
        chat_repository,
        user_repository,
        Arc::new(MessageRepository::new(db.clone())),
        attachment_repository,
        connectivity_state,
        identity_state,
        session_state,
        authenticator,
        settings.facade.session_token.clone(),
    ));

    Ok(Started { facade, handles })
}

fn open_error(err: sea_orm::DbErr) -> BootError {
    match &err {
        sea_orm::DbErr::Custom(message) if message == DatabaseProvider::KEY_VERIFICATION_FAILED => {
            BootError::WrongKey
        }
        _ => BootError::Failed(format!("Failed to initialize database connection: {err}")),
    }
}
