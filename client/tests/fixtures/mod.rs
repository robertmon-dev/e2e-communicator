#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use orion::aead::SecretKey;
use sea_orm::{ActiveModelTrait, Set};
use sea_orm_migration::MigratorTrait;
use uuid::Uuid;

use client::adapters::r#in::ipc::facade::Facade;
use client::adapters::out::keys::DeviceKeyStore;
use client::adapters::out::persistance::entities::{chat, message, user};
use client::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use client::adapters::out::persistance::repositories::chat::ChatRepository;
use client::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use client::adapters::out::persistance::repositories::key::KeyRepository;
use client::adapters::out::persistance::repositories::local_identity::LocalIdentityRepository;
use client::adapters::out::persistance::repositories::message::MessageRepository;
use client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use client::adapters::out::persistance::repositories::user::UserRepository;
use client::domain::dto::errors::TransportError;
use client::domain::dto::identity::{LinkToken, RegisteredDevice};
use client::domain::dto::local_identity::LocalIdentity;
use client::domain::services::authenticator::{Authenticator, IdentityTransport};
use client::domain::services::encryptor::Encryptor;
use client::domain::services::exchanger::Exchanger;
use client::domain::services::inbox::{EnvelopeSubscription, InboundStream};
use client::domain::services::messenger::Messenger;
use client::domain::state::connectivity::{ConnectivityState, ConnectivityStatus};
use client::domain::state::identity::{IdentityState, IdentityStatus};
use client::domain::state::session::{SessionState, SessionToken};
use common::identity::{
    DevicePublicKeys, SIGNATURE_LEN, canonical_challenge_bytes, canonical_device_bytes,
    encode_token, random_bytes, verify,
};

const TEST_RETRY_DELAY: Duration = Duration::from_millis(10);

pub struct NoopSubscription;

impl EnvelopeSubscription for NoopSubscription {
    async fn subscribe(
        &self,
        _user_id: Uuid,
        _device_id: Uuid,
    ) -> Result<InboundStream, TransportError> {
        unimplemented!("not exercised by these tests")
    }

    async fn ack(
        &self,
        _user_id: Uuid,
        _device_id: Uuid,
        _delivery_token: &str,
    ) -> Result<(), TransportError> {
        Ok(())
    }
}

pub async fn setup_db() -> Arc<sea_orm::DatabaseConnection> {
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    migration::Migrator::up(&db, None).await.unwrap();
    Arc::new(db)
}

pub async fn seed_user(db: &sea_orm::DatabaseConnection, id: Uuid) {
    let now = Utc::now();
    user::ActiveModel {
        id: Set(id),
        name: Set(None),
        surname: Set(None),
        nickname: Set(id.to_string()),
        phone: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
}

pub async fn seed_chat(db: &sea_orm::DatabaseConnection, id: Uuid) {
    let now = Utc::now();
    chat::ActiveModel {
        id: Set(id),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
}

pub async fn seed_message(
    db: &sea_orm::DatabaseConnection,
    id: Uuid,
    author_id: Uuid,
    chat_id: Uuid,
) {
    let now = Utc::now();
    message::ActiveModel {
        id: Set(id),
        author_id: Set(author_id),
        chat_id: Set(chat_id),
        available_to: Set(None),
        hidden: Set(false),
        is_reply: Set(false),
        reply_to_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
}

pub fn identity_with(user_id: Uuid, device_id: Uuid) -> LocalIdentity {
    LocalIdentity {
        user_id,
        device_id,
        agreement_public_key: vec![1; 32],
        signing_public_key: vec![2; 32],
        device_cert: vec![3; 64],
        created_at: Utc::now(),
    }
}

pub fn registered_identity(user_id: Uuid) -> Arc<IdentityState> {
    Arc::new(IdentityState::new(IdentityStatus::Registered(
        identity_with(user_id, Uuid::new_v4()),
    )))
}

pub struct FakeIdentityTransport {
    pub registered: Mutex<HashMap<(Uuid, Uuid), DevicePublicKeys>>,
    pub taken_users: Mutex<HashSet<Uuid>>,
    pub challenges: Mutex<HashMap<(Uuid, Uuid), [u8; 32]>>,
    pub authenticate_failures: Mutex<VecDeque<TransportError>>,
    pub link_tokens: Mutex<HashMap<String, Uuid>>,
    pub revoked: Mutex<Vec<Uuid>>,
    pub calls: Mutex<Vec<String>>,
    pub session_ttl: Duration,
}

impl FakeIdentityTransport {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::with_session_ttl(Duration::from_secs(3600)))
    }

    pub fn with_session_ttl(session_ttl: Duration) -> Self {
        Self {
            registered: Mutex::new(HashMap::new()),
            taken_users: Mutex::new(HashSet::new()),
            challenges: Mutex::new(HashMap::new()),
            authenticate_failures: Mutex::new(VecDeque::new()),
            link_tokens: Mutex::new(HashMap::new()),
            revoked: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
            session_ttl,
        }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    pub fn count(&self, name: &str) -> usize {
        self.calls().iter().filter(|c| c.as_str() == name).count()
    }

    fn record(&self, name: &str) {
        self.calls.lock().unwrap().push(name.to_string());
    }
}

#[async_trait]
impl IdentityTransport for FakeIdentityTransport {
    async fn register_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
    ) -> Result<(), TransportError> {
        self.record("register_device");
        if self.taken_users.lock().unwrap().contains(&user_id) {
            return Err(TransportError::AlreadyExists("taken".to_string()));
        }
        verify(
            &keys.signing_key,
            &canonical_device_bytes(user_id, device_id, keys),
            self_proof,
        )
        .map_err(|_| TransportError::SendFailed("bad self proof".to_string()))?;
        self.registered
            .lock()
            .unwrap()
            .insert((user_id, device_id), keys.clone());
        Ok(())
    }

    async fn create_link_token(
        &self,
        user_id: Uuid,
        _device_id: Uuid,
    ) -> Result<LinkToken, TransportError> {
        self.record("create_link_token");
        let token = encode_token(&random_bytes::<16>().unwrap());
        self.link_tokens
            .lock()
            .unwrap()
            .insert(token.clone(), user_id);
        Ok(LinkToken {
            user_id,
            token,
            expires_at: Utc::now() + ChronoDuration::minutes(5),
        })
    }

    async fn link_device(
        &self,
        token: &str,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
    ) -> Result<Uuid, TransportError> {
        self.record("link_device");
        let user_id = self
            .link_tokens
            .lock()
            .unwrap()
            .remove(token)
            .ok_or_else(|| TransportError::NotFound("link token".to_string()))?;
        verify(
            &keys.signing_key,
            &canonical_device_bytes(user_id, device_id, keys),
            self_proof,
        )
        .map_err(|_| TransportError::SendFailed("bad self proof".to_string()))?;
        self.registered
            .lock()
            .unwrap()
            .insert((user_id, device_id), keys.clone());
        Ok(user_id)
    }

    async fn revoke_device(
        &self,
        user_id: Uuid,
        _device_id: Uuid,
        target_device_id: Uuid,
    ) -> Result<(), TransportError> {
        self.record("revoke_device");
        self.revoked.lock().unwrap().push(target_device_id);
        self.registered
            .lock()
            .unwrap()
            .remove(&(user_id, target_device_id));
        Ok(())
    }

    async fn lookup_devices(
        &self,
        _user_id: Uuid,
        _device_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<Vec<RegisteredDevice>, TransportError> {
        self.record("lookup_devices");
        Ok(self
            .registered
            .lock()
            .unwrap()
            .iter()
            .filter(|((user, _), _)| *user == target_user_id)
            .map(|((_, device), keys)| RegisteredDevice {
                device_id: *device,
                keys: keys.clone(),
                registered_at: Utc::now(),
            })
            .collect())
    }

    async fn challenge(&self, user_id: Uuid, device_id: Uuid) -> Result<[u8; 32], TransportError> {
        self.record("challenge");
        if !self
            .registered
            .lock()
            .unwrap()
            .contains_key(&(user_id, device_id))
        {
            return Err(TransportError::Unauthenticated(
                "unknown device".to_string(),
            ));
        }
        let nonce = random_bytes::<32>().unwrap();
        self.challenges
            .lock()
            .unwrap()
            .insert((user_id, device_id), nonce);
        Ok(nonce)
    }

    async fn authenticate(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        signature: &[u8; SIGNATURE_LEN],
    ) -> Result<SessionToken, TransportError> {
        self.record("authenticate");
        if let Some(err) = self.authenticate_failures.lock().unwrap().pop_front() {
            return Err(err);
        }
        let nonce = self
            .challenges
            .lock()
            .unwrap()
            .remove(&(user_id, device_id))
            .ok_or_else(|| TransportError::Unauthenticated("no challenge".to_string()))?;
        let keys = self
            .registered
            .lock()
            .unwrap()
            .get(&(user_id, device_id))
            .cloned()
            .ok_or_else(|| TransportError::Unauthenticated("unknown device".to_string()))?;
        verify(
            &keys.signing_key,
            &canonical_challenge_bytes(user_id, device_id, &nonce),
            signature,
        )
        .map_err(|_| TransportError::Unauthenticated("bad signature".to_string()))?;

        Ok(SessionToken {
            token: encode_token(&random_bytes::<32>().unwrap()),
            expires_at: Utc::now() + ChronoDuration::from_std(self.session_ttl).unwrap(),
        })
    }
}

pub struct AuthHarness {
    pub authenticator: Arc<Authenticator>,
    pub transport: Arc<FakeIdentityTransport>,
    pub identity: Arc<IdentityState>,
    pub session: Arc<SessionState>,
    pub connectivity: Arc<ConnectivityState>,
    pub identity_repository: Arc<LocalIdentityRepository>,
    pub device_keys: Arc<DeviceKeyStore>,
}

impl AuthHarness {
    pub fn reconfigure<F>(&mut self, extra: F)
    where
        F: FnOnce(Authenticator) -> Authenticator,
    {
        let base = Authenticator::new(
            self.transport.clone(),
            self.device_keys.clone(),
            self.identity_repository.clone(),
            self.identity.clone(),
            self.session.clone(),
            self.connectivity.clone(),
        )
        .with_retry_delay(TEST_RETRY_DELAY);

        self.authenticator = Arc::new(extra(base));
    }
}

pub fn device_key_store(db: &Arc<sea_orm::DatabaseConnection>) -> Arc<DeviceKeyStore> {
    Arc::new(DeviceKeyStore::new(
        Arc::new(DeviceKeyRepository::new(db.clone())),
        Encryptor::new(SecretKey::default()),
    ))
}

pub async fn build_authenticator(
    db: &Arc<sea_orm::DatabaseConnection>,
    transport: Arc<FakeIdentityTransport>,
    initial: IdentityStatus,
    connectivity: ConnectivityStatus,
) -> AuthHarness {
    let device_keys = device_key_store(db);
    let identity_repository = Arc::new(LocalIdentityRepository::new(
        db.clone(),
        device_keys.clone(),
    ));
    let identity = Arc::new(IdentityState::new(initial));
    let session = Arc::new(SessionState::default());
    let connectivity = Arc::new(ConnectivityState::new(connectivity));

    let authenticator = Arc::new(
        Authenticator::new(
            transport.clone(),
            device_keys.clone(),
            identity_repository.clone(),
            identity.clone(),
            session.clone(),
            connectivity.clone(),
        )
        .with_retry_delay(TEST_RETRY_DELAY),
    );

    AuthHarness {
        authenticator,
        transport,
        identity,
        session,
        connectivity,
        identity_repository,
        device_keys,
    }
}

pub async fn registered_harness(
    db: &Arc<sea_orm::DatabaseConnection>,
    transport: Arc<FakeIdentityTransport>,
) -> (AuthHarness, LocalIdentity) {
    let harness = build_authenticator(
        db,
        transport,
        IdentityStatus::Unregistered,
        ConnectivityStatus::Online,
    )
    .await;
    let identity = harness.authenticator.register_new().await.unwrap();
    (harness, identity)
}

pub async fn wait_until<F: Fn() -> bool>(what: &str, condition: F) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for: {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub struct FacadeHarness {
    pub facade: Arc<Facade>,
    pub auth: AuthHarness,
}

pub async fn build_facade(
    db: &Arc<sea_orm::DatabaseConnection>,
    initial: IdentityStatus,
    session_token: Option<String>,
) -> FacadeHarness {
    let auth = build_authenticator(
        db,
        FakeIdentityTransport::new(),
        initial,
        ConnectivityStatus::Online,
    )
    .await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let attachment_repository = Arc::new(AttachmentRepository::new(db.clone()));
    let user_repository = Arc::new(UserRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository, SecretKey::default()));
    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let chat_repository = Arc::new(ChatRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));
    let messenger = Arc::new(Messenger::new(
        message_repository.clone(),
        chat_repository.clone(),
        outbox_repository,
        exchanger,
    ));

    let facade = Arc::new(Facade::new(
        messenger,
        chat_repository,
        user_repository,
        message_repository,
        attachment_repository,
        auth.connectivity.clone(),
        auth.identity.clone(),
        auth.session.clone(),
        auth.authenticator.clone(),
        session_token,
    ));

    FacadeHarness { facade, auth }
}
