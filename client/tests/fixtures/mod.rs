#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use orion::aead::SecretKey;
use orion::hazardous::ecc::x25519::{PrivateKey as X25519PrivateKey, PublicKey as X25519PublicKey};
use sea_orm::{ActiveModelTrait, Set};
use sea_orm_migration::MigratorTrait;
use std::collections::BTreeMap;
use uuid::Uuid;

use client::adapters::r#in::ipc::facade::{Facade, FacadeDeps};
use client::adapters::out::keys::{DeviceKeyStore, DeviceKeyStoreRepository, UserIdentityStore};
use client::adapters::out::persistance::entities::{chat, message, user};
use client::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use client::adapters::out::persistance::repositories::chat::ChatRepository;
use client::adapters::out::persistance::repositories::chat::create::Create as ChatCreate;
use client::adapters::out::persistance::repositories::chat::update::Update as ChatUpdate;
use client::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use client::adapters::out::persistance::repositories::invitation::InvitationRepository;
use client::adapters::out::persistance::repositories::key::KeyRepository;
use client::adapters::out::persistance::repositories::local_identity::LocalIdentityRepository;
use client::adapters::out::persistance::repositories::message::MessageRepository;
use client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use client::adapters::out::persistance::repositories::peer_device::PeerDeviceRepository;
use client::adapters::out::persistance::repositories::user::UserRepository;
use client::adapters::out::persistance::repositories::user_identity::UserIdentityRepository;
use client::domain::dto::chat::{Chat, ChatType};
use client::domain::dto::errors::TransportError;
use client::domain::dto::identity::{IssuedLinkToken, RegisteredDevice, UserDirectory};
use client::domain::dto::invitation::Invitation;
use client::domain::dto::local_identity::LocalIdentity;
use client::domain::dto::outbox::OutboxEntry;
use client::domain::dto::permission::MemberRole;
use client::domain::services::authenticator::{Authenticator, IdentityTransport};
use client::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};
use client::domain::services::exchanger::Exchanger;
use client::domain::services::inbox::{EnvelopeSubscription, InboundStream};
use client::domain::services::inviter::Inviter;
use client::domain::services::messenger::Messenger;
use client::domain::services::peer_directory::PeerDirectory;
use client::domain::state::connectivity::{ConnectivityState, ConnectivityStatus};
use client::domain::state::identity::{IdentityState, IdentityStatus};
use client::domain::state::session::{SessionState, SessionToken};
use common::envelope::EventType;
use common::identity::{
    DevicePublicKeys, KEY_LEN, SIGNATURE_LEN, canonical_challenge_bytes, canonical_device_bytes,
    canonical_device_cert_bytes, encode_contact_code, encode_token, fixed, random_bytes, sign,
    signing_public_key, user_fingerprint, verify,
};
use serde::de::DeserializeOwned;

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
        signing_public_key: Set(None),
        verified_at: Set(None),
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
    pub user_keys: Mutex<HashMap<Uuid, [u8; KEY_LEN]>>,
    pub certs: Mutex<HashMap<(Uuid, Uuid), [u8; SIGNATURE_LEN]>>,
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
            user_keys: Mutex::new(HashMap::new()),
            certs: Mutex::new(HashMap::new()),
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

    fn seed_device(
        &self,
        user_id: Uuid,
        user_seed: &[u8; KEY_LEN],
        cert_seed: &[u8; KEY_LEN],
    ) -> Uuid {
        let device_id = Uuid::new_v4();
        let device_keys = DevicePublicKeys {
            signing_key: random_bytes::<KEY_LEN>().unwrap(),
            agreement_key: random_bytes::<KEY_LEN>().unwrap(),
        };
        let cert = sign(
            cert_seed,
            &canonical_device_cert_bytes(user_id, device_id, &device_keys),
        );

        self.user_keys
            .lock()
            .unwrap()
            .insert(user_id, signing_public_key(user_seed));
        self.certs
            .lock()
            .unwrap()
            .insert((user_id, device_id), cert);
        self.registered
            .lock()
            .unwrap()
            .insert((user_id, device_id), device_keys);

        device_id
    }

    pub fn seed_certified_device(&self, user_id: Uuid, user_seed: &[u8; KEY_LEN]) -> Uuid {
        self.seed_device(user_id, user_seed, user_seed)
    }

    pub fn seed_device_with_cert_from(
        &self,
        user_id: Uuid,
        user_seed: &[u8; KEY_LEN],
        cert_seed: &[u8; KEY_LEN],
    ) -> Uuid {
        self.seed_device(user_id, user_seed, cert_seed)
    }

    pub fn seed_device_with_keys(
        &self,
        user_id: Uuid,
        user_seed: &[u8; KEY_LEN],
        device_id: Uuid,
        keys: &DevicePublicKeys,
    ) -> Uuid {
        let cert = sign(
            user_seed,
            &canonical_device_cert_bytes(user_id, device_id, keys),
        );

        self.user_keys
            .lock()
            .unwrap()
            .insert(user_id, signing_public_key(user_seed));
        self.certs
            .lock()
            .unwrap()
            .insert((user_id, device_id), cert);
        self.registered
            .lock()
            .unwrap()
            .insert((user_id, device_id), keys.clone());

        device_id
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
        user_signing_key: &[u8; KEY_LEN],
        device_cert: &[u8; SIGNATURE_LEN],
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
        verify(
            user_signing_key,
            &canonical_device_cert_bytes(user_id, device_id, keys),
            device_cert,
        )
        .map_err(|_| TransportError::SendFailed("bad device cert".to_string()))?;

        {
            let mut user_keys = self.user_keys.lock().unwrap();
            if let Some(existing) = user_keys.get(&user_id) {
                if existing != user_signing_key {
                    return Err(TransportError::AlreadyExists(
                        "user key mismatch".to_string(),
                    ));
                }
            } else {
                user_keys.insert(user_id, *user_signing_key);
            }
        }
        self.certs
            .lock()
            .unwrap()
            .insert((user_id, device_id), *device_cert);
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
    ) -> Result<IssuedLinkToken, TransportError> {
        self.record("create_link_token");
        let token = encode_token(&random_bytes::<16>().unwrap());
        self.link_tokens
            .lock()
            .unwrap()
            .insert(token.clone(), user_id);
        Ok(IssuedLinkToken {
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
        user_signing_key: &[u8; KEY_LEN],
        device_cert: &[u8; SIGNATURE_LEN],
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
        verify(
            user_signing_key,
            &canonical_device_cert_bytes(user_id, device_id, keys),
            device_cert,
        )
        .map_err(|_| TransportError::SendFailed("bad device cert".to_string()))?;

        {
            let mut user_keys = self.user_keys.lock().unwrap();
            if let Some(existing) = user_keys.get(&user_id) {
                if existing != user_signing_key {
                    return Err(TransportError::AlreadyExists(
                        "user key mismatch".to_string(),
                    ));
                }
            } else {
                user_keys.insert(user_id, *user_signing_key);
            }
        }
        self.certs
            .lock()
            .unwrap()
            .insert((user_id, device_id), *device_cert);
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
    ) -> Result<UserDirectory, TransportError> {
        self.record("lookup_devices");
        let user_signing_key = self
            .user_keys
            .lock()
            .unwrap()
            .get(&target_user_id)
            .copied()
            .ok_or_else(|| TransportError::NotFound("user signing key".to_string()))?;

        let certs = self.certs.lock().unwrap();
        let devices = self
            .registered
            .lock()
            .unwrap()
            .iter()
            .filter(|((user, _), _)| *user == target_user_id)
            .map(|((user, device), keys)| RegisteredDevice {
                device_id: *device,
                keys: keys.clone(),
                registered_at: Utc::now(),
                cert: certs
                    .get(&(*user, *device))
                    .copied()
                    .unwrap_or([0u8; SIGNATURE_LEN]),
            })
            .collect();

        Ok(UserDirectory {
            user_signing_key,
            devices,
        })
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
    pub user_identity: Arc<UserIdentityStore>,
}

impl AuthHarness {
    pub fn reconfigure<F>(&mut self, extra: F)
    where
        F: FnOnce(Authenticator) -> Authenticator,
    {
        let base = Authenticator::new(
            self.transport.clone(),
            self.device_keys.clone(),
            self.user_identity.clone(),
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

#[allow(clippy::type_complexity)]
pub fn inbox_support(
    db: &Arc<sea_orm::DatabaseConnection>,
    identity: &Arc<IdentityState>,
    exchanger: &Arc<Exchanger>,
    outbox_repository: &Arc<OutboxRepository>,
) -> (
    Arc<PeerDirectory>,
    Arc<InvitationRepository>,
    Arc<ChatRepository>,
    Arc<UserRepository>,
    Arc<DeviceKeyStore>,
    Arc<KeyRepository>,
    Arc<Inviter>,
) {
    let transport = FakeIdentityTransport::new();
    let peer_devices = Arc::new(PeerDeviceRepository::new(db.clone()));
    let peer_directory = Arc::new(PeerDirectory::new(
        transport,
        peer_devices,
        identity.clone(),
    ));
    let invitation_repository = Arc::new(InvitationRepository::new(db.clone()));
    let chat_repository = Arc::new(ChatRepository::new(db.clone()));
    let user_repository = Arc::new(UserRepository::new(db.clone()));
    let device_keys = device_key_store(db);
    let key_repository = Arc::new(KeyRepository::new(db.clone()));

    let inviter = Arc::new(Inviter::new(
        peer_directory.clone(),
        exchanger.clone(),
        device_keys.clone(),
        chat_repository.clone(),
        user_repository.clone(),
        invitation_repository.clone(),
        outbox_repository.clone(),
        key_repository.clone(),
        identity.clone(),
    ));

    (
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository,
        inviter,
    )
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
    let user_identity = Arc::new(UserIdentityStore::new(
        Arc::new(UserIdentityRepository::new(db.clone())),
        Encryptor::new(SecretKey::default()),
    ));
    let identity = Arc::new(IdentityState::new(initial));
    let session = Arc::new(SessionState::default());
    let connectivity = Arc::new(ConnectivityState::new(connectivity));

    let authenticator = Arc::new(
        Authenticator::new(
            transport.clone(),
            device_keys.clone(),
            user_identity.clone(),
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
        user_identity,
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
    db: Arc<sea_orm::DatabaseConnection>,
}

pub struct SeededContact {
    pub user_id: Uuid,
    pub user_seed: [u8; KEY_LEN],
    pub device_ids: Vec<Uuid>,
    pub contact_code: String,
}

impl FacadeHarness {
    pub async fn seed_contact(&self, devices: usize) -> SeededContact {
        let user_id = Uuid::new_v4();
        seed_user(&self.db, user_id).await;

        let user_seed = random_bytes::<KEY_LEN>().unwrap();
        let mut device_ids = Vec::with_capacity(devices);
        for _ in 0..devices {
            device_ids.push(
                self.auth
                    .transport
                    .seed_certified_device(user_id, &user_seed),
            );
        }

        let fingerprint = user_fingerprint(&signing_public_key(&user_seed));
        let contact_code = encode_contact_code(user_id, &fingerprint);

        SeededContact {
            user_id,
            user_seed,
            device_ids,
            contact_code,
        }
    }
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
    let exchanger = Arc::new(Exchanger::new(key_repository.clone(), SecretKey::default()));
    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let chat_repository = Arc::new(ChatRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));
    let messenger = Arc::new(Messenger::new(
        message_repository.clone(),
        chat_repository.clone(),
        outbox_repository.clone(),
        key_repository.clone(),
        exchanger.clone(),
    ));

    let peer_device_repository = Arc::new(PeerDeviceRepository::new(db.clone()));
    let peer_directory = Arc::new(PeerDirectory::new(
        auth.transport.clone(),
        peer_device_repository.clone(),
        auth.identity.clone(),
    ));
    let invitation_repository = Arc::new(InvitationRepository::new(db.clone()));

    let inviter = Arc::new(Inviter::new(
        peer_directory.clone(),
        exchanger,
        auth.device_keys.clone(),
        chat_repository.clone(),
        user_repository.clone(),
        invitation_repository.clone(),
        outbox_repository,
        key_repository,
        auth.identity.clone(),
    ));

    let facade = Arc::new(Facade::new(FacadeDeps {
        messenger,
        inviter,
        peer_directory,
        chat_repository,
        message_repository,
        attachment_repository,
        user_repository,
        invitation_repository,
        peer_device_repository,
        user_identity: auth.user_identity.clone(),
        connectivity: auth.connectivity.clone(),
        identity: auth.identity.clone(),
        session: auth.session.clone(),
        authenticator: auth.authenticator.clone(),
        session_token,
    }));

    FacadeHarness {
        facade,
        auth,
        db: db.clone(),
    }
}

pub struct SeededPeer {
    pub user_id: Uuid,
    pub user_seed: [u8; KEY_LEN],
    pub device_ids: Vec<Uuid>,
}

pub struct InviterHarness {
    pub inviter: Arc<Inviter>,
    pub chat_repository: Arc<ChatRepository>,
    pub outbox_repository: Arc<OutboxRepository>,
    pub user_repository: Arc<UserRepository>,
    pub invitation_repository: Arc<InvitationRepository>,
    pub key_repository: Arc<KeyRepository>,
    pub peer_devices: Arc<PeerDeviceRepository>,
    pub transport: Arc<FakeIdentityTransport>,
    pub local_user_id: Uuid,
    pub local_device_id: Uuid,
    pub device_keys: Arc<DeviceKeyStore>,
    pub identity: Arc<IdentityState>,
    pub exchanger: Arc<Exchanger>,
    db: Arc<sea_orm::DatabaseConnection>,
}

impl InviterHarness {
    pub fn seed_peer_devices_only(&self, count: usize) -> SeededPeer {
        let user_id = Uuid::new_v4();
        let user_seed = random_bytes::<KEY_LEN>().unwrap();
        let mut device_ids = Vec::with_capacity(count);
        for _ in 0..count {
            device_ids.push(self.transport.seed_certified_device(user_id, &user_seed));
        }

        SeededPeer {
            user_id,
            user_seed,
            device_ids,
        }
    }

    pub async fn seed_peer_with_devices(&self, count: usize) -> SeededPeer {
        let user_id = Uuid::new_v4();
        seed_user(&self.db, user_id).await;

        let user_seed = random_bytes::<KEY_LEN>().unwrap();
        let mut device_ids = Vec::with_capacity(count);
        for _ in 0..count {
            device_ids.push(self.transport.seed_certified_device(user_id, &user_seed));
        }

        SeededPeer {
            user_id,
            user_seed,
            device_ids,
        }
    }

    pub async fn seed_incoming_invitation(&self) -> (Invitation, SeededPeer) {
        let peer = self.seed_peer_with_devices(1).await;

        let peer_directory = PeerDirectory::new(
            self.transport.clone(),
            self.peer_devices.clone(),
            self.identity.clone(),
        );
        peer_directory.resolve(peer.user_id, None).await.unwrap();

        let mut members = BTreeMap::new();
        members.insert(self.local_user_id, MemberRole::Admin);
        members.insert(peer.user_id, MemberRole::Admin);

        let chat = Chat::new(
            ChatType::Direct,
            None,
            members,
            BTreeMap::new(),
            None,
            false,
        );
        self.chat_repository.create(chat.clone()).await.unwrap();

        let peer_private_key = X25519PrivateKey::generate();
        let peer_public_key = X25519PublicKey::try_from(&peer_private_key).unwrap();
        self.exchanger
            .store_peer_key(
                chat.id,
                peer.user_id,
                peer.device_ids[0],
                peer_public_key.to_bytes().to_vec(),
            )
            .await
            .unwrap();

        let invitation =
            Invitation::incoming(Uuid::new_v4(), chat.id, peer.user_id, self.local_user_id);
        self.invitation_repository
            .upsert(invitation.clone())
            .await
            .unwrap();

        (invitation, peer)
    }

    pub async fn seed_member_of(&self, chat_id: Uuid, nickname: &str) -> Uuid {
        let user_id = Uuid::new_v4();
        let now = Utc::now();
        user::ActiveModel {
            id: Set(user_id),
            name: Set(None),
            surname: Set(None),
            nickname: Set(nickname.to_string()),
            phone: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            signing_public_key: Set(None),
            verified_at: Set(None),
        }
        .insert(self.db.as_ref())
        .await
        .unwrap();

        let chat = self.chat_repository.get(chat_id).await.unwrap();
        let mut state = chat.get_latest().unwrap().clone();
        state.members.insert(user_id, MemberRole::Member);
        self.chat_repository
            .add_history_record(chat_id, state)
            .await
            .unwrap();

        user_id
    }

    pub async fn seed_member_with_devices(
        &self,
        chat_id: Uuid,
        nickname: &str,
        devices: usize,
    ) -> SeededPeer {
        let user_id = self.seed_member_of(chat_id, nickname).await;

        let user_seed = random_bytes::<KEY_LEN>().unwrap();
        let mut device_ids = Vec::with_capacity(devices);
        for _ in 0..devices {
            device_ids.push(self.transport.seed_certified_device(user_id, &user_seed));
        }

        let peer_directory = PeerDirectory::new(
            self.transport.clone(),
            self.peer_devices.clone(),
            self.identity.clone(),
        );
        peer_directory.resolve(user_id, None).await.unwrap();

        SeededPeer {
            user_id,
            user_seed,
            device_ids,
        }
    }

    pub async fn seed_group_where_local_is_member(&self) -> Chat {
        let mut members = BTreeMap::new();
        members.insert(self.local_user_id, MemberRole::Member);

        let chat = Chat::new(
            ChatType::Group,
            Some("Group".to_string()),
            members,
            BTreeMap::new(),
            None,
            true,
        );
        self.chat_repository.create(chat.clone()).await.unwrap();

        chat
    }

    pub async fn last_entry_for(
        &self,
        recipient: Uuid,
        event_type: EventType,
    ) -> Option<OutboxEntry> {
        let due = self.outbox_repository.list_due(1000).await.unwrap();
        due.into_iter()
            .filter(|entry| entry.recipient_id == recipient && entry.event_type == event_type)
            .max_by_key(|entry| entry.created_at)
    }

    pub async fn last_payload_for<P: DeserializeOwned>(
        &self,
        recipient: Uuid,
        event_type: EventType,
    ) -> Option<P> {
        let entry = self.last_entry_for(recipient, event_type).await?;

        let devices = self.peer_devices.list_by_user(recipient).await.unwrap();
        let device = devices
            .into_iter()
            .find(|d| d.device_id == entry.recipient_device_id)?;

        let agreement_key: [u8; KEY_LEN] = fixed(&device.agreement_public_key).ok()?;
        let shared_key = self
            .device_keys
            .derive_device_shared_key(
                self.local_device_id,
                entry.recipient_device_id,
                &agreement_key,
            )
            .await
            .ok()?;

        let plaintext = Encryptor::decrypt_with_derived_key(&shared_key, &entry.ciphertext).ok()?;
        serde_json::from_slice(&plaintext).ok()
    }
}

pub async fn inviter_harness(db: &Arc<sea_orm::DatabaseConnection>) -> InviterHarness {
    let local_user_id = Uuid::new_v4();
    let local_device_id = Uuid::new_v4();
    seed_user(db, local_user_id).await;

    let device_keys = device_key_store(db);
    let local_keys = device_keys
        .get_or_create_keys(local_device_id)
        .await
        .unwrap();

    let local_identity = LocalIdentity::new(
        local_user_id,
        local_device_id,
        &local_keys,
        vec![9; SIGNATURE_LEN],
    );
    let identity = Arc::new(IdentityState::new(IdentityStatus::Registered(
        local_identity,
    )));

    let transport = FakeIdentityTransport::new();
    let peer_devices = Arc::new(PeerDeviceRepository::new(db.clone()));
    let peer_directory = Arc::new(PeerDirectory::new(
        transport.clone(),
        peer_devices.clone(),
        identity.clone(),
    ));

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository.clone(), SecretKey::default()));
    let chat_repository = Arc::new(ChatRepository::new(db.clone()));
    let user_repository = Arc::new(UserRepository::new(db.clone()));
    let invitation_repository = Arc::new(InvitationRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let inviter = Arc::new(Inviter::new(
        peer_directory,
        exchanger.clone(),
        device_keys.clone(),
        chat_repository.clone(),
        user_repository.clone(),
        invitation_repository.clone(),
        outbox_repository.clone(),
        key_repository.clone(),
        identity.clone(),
    ));

    InviterHarness {
        inviter,
        chat_repository,
        outbox_repository,
        user_repository,
        invitation_repository,
        key_repository,
        peer_devices,
        transport,
        local_user_id,
        local_device_id,
        device_keys,
        identity,
        exchanger,
        db: db.clone(),
    }
}
