#![allow(dead_code)]

use std::sync::Arc;

use common::envelope::{EncryptedEnvelope, EventType};
use common::identity::{
    DevicePublicKeys, KEY_LEN, NONCE_LEN, SIGNATURE_LEN, canonical_challenge_bytes,
    canonical_device_bytes, canonical_device_cert_bytes, generate_signing_seed, random_bytes, sign,
    signing_public_key,
};
use redis::AsyncCommands;
use server::core::initializers::redis::MailboxStore;
use server::core::initializers::settings::{AuthSettings, RedisSettings};
use server::domain::services::mailbox::Mailbox;
use server::domain::services::registry::{Registry, Session};
use tonic::Request;
use uuid::Uuid;

pub fn redis_settings() -> RedisSettings {
    RedisSettings {
        sentinel_nodes: vec![
            "localhost:26379".to_string(),
            "localhost:26380".to_string(),
            "localhost:26381".to_string(),
        ],
        service_name: "mymaster".to_string(),
        password: None,
    }
}

pub fn store() -> Arc<MailboxStore> {
    MailboxStore::new(&redis_settings())
        .expect("failed to build MailboxStore - is `make redis-up` running?")
}

pub fn mailbox_with(store: Arc<MailboxStore>) -> Arc<Mailbox> {
    Arc::new(Mailbox::new(store))
}

pub fn mailbox() -> Arc<Mailbox> {
    mailbox_with(store())
}

pub fn test_envelope(recipient_id: Uuid) -> EncryptedEnvelope {
    EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id: Uuid::new_v4(),
        sender_id: Uuid::new_v4(),
        sender_device_id: Uuid::new_v4(),
        recipient_id,
        recipient_device_id: None,
        created_at: chrono::Utc::now(),
        event_type: EventType::Message,
        notify: true,
        ciphertext: vec![1, 2, 3],
    }
}

pub async fn register_devices(mailbox: &Mailbox, user_id: Uuid, device_ids: &[Uuid]) {
    let mut conn = mailbox.blocking_connection().await.unwrap();
    for device_id in device_ids {
        let _: usize = conn
            .sadd(format!("devices:{user_id}"), device_id.to_string())
            .await
            .unwrap();
    }
}

pub async fn stream_len(mailbox: &Mailbox, user_id: Uuid, device_id: Uuid) -> usize {
    let mut conn = mailbox.blocking_connection().await.unwrap();
    conn.xlen(format!("mailbox:{user_id}:{device_id}"))
        .await
        .unwrap()
}

pub fn registry() -> Arc<Registry> {
    registry_with(AuthSettings::default())
}

pub fn registry_with(settings: AuthSettings) -> Arc<Registry> {
    Arc::new(Registry::new(store(), settings))
}

pub fn registry_on(store: Arc<MailboxStore>, settings: AuthSettings) -> Arc<Registry> {
    Arc::new(Registry::new(store, settings))
}

fn user_signing_seed(user_id: Uuid) -> [u8; KEY_LEN] {
    let mut seed = [0u8; KEY_LEN];
    let bytes = user_id.as_bytes();
    seed[..16].copy_from_slice(bytes);
    seed[16..].copy_from_slice(bytes);
    seed
}

pub struct TestDevice {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub seed: [u8; 32],
    pub keys: DevicePublicKeys,
}

impl TestDevice {
    pub fn new(user_id: Uuid) -> Self {
        let seed = *generate_signing_seed().unwrap();
        Self {
            user_id,
            device_id: Uuid::new_v4(),
            keys: DevicePublicKeys {
                signing_key: signing_public_key(&seed),
                agreement_key: random_bytes::<32>().unwrap(),
            },
            seed,
        }
    }

    pub fn proof(&self) -> [u8; SIGNATURE_LEN] {
        sign(
            &self.seed,
            &canonical_device_bytes(self.user_id, self.device_id, &self.keys),
        )
    }

    pub fn sign_challenge(&self, nonce: &[u8; NONCE_LEN]) -> [u8; SIGNATURE_LEN] {
        sign(
            &self.seed,
            &canonical_challenge_bytes(self.user_id, self.device_id, nonce),
        )
    }

    pub fn user_signing_key(&self) -> [u8; KEY_LEN] {
        signing_public_key(&user_signing_seed(self.user_id))
    }

    pub fn cert(&self) -> [u8; SIGNATURE_LEN] {
        sign(
            &user_signing_seed(self.user_id),
            &canonical_device_cert_bytes(self.user_id, self.device_id, &self.keys),
        )
    }
}

pub async fn register(registry: &Registry, device: &TestDevice) {
    registry
        .register_device(
            device.user_id,
            device.device_id,
            &device.keys,
            &device.proof(),
            &device.user_signing_key(),
            &device.cert(),
        )
        .await
        .unwrap();
}

pub async fn login(registry: &Registry, device: &TestDevice) -> Session {
    let nonce = registry
        .issue_challenge(device.user_id, device.device_id)
        .await
        .unwrap();
    registry
        .authenticate(
            device.user_id,
            device.device_id,
            &device.sign_challenge(&nonce),
        )
        .await
        .unwrap()
}

pub fn with_bearer<T>(mut request: Request<T>, token: &str) -> Request<T> {
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("ascii token"),
    );
    request
}
