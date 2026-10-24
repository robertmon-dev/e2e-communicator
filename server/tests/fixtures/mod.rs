#![allow(dead_code)]

use std::sync::Arc;

use common::identity::{
    DevicePublicKeys, NONCE_LEN, SIGNATURE_LEN, canonical_challenge_bytes, canonical_device_bytes,
    generate_signing_seed, random_bytes, sign, signing_public_key,
};
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

pub fn registry() -> Arc<Registry> {
    registry_with(AuthSettings::default())
}

pub fn registry_with(settings: AuthSettings) -> Arc<Registry> {
    Arc::new(Registry::new(store(), settings))
}

pub fn registry_on(store: Arc<MailboxStore>, settings: AuthSettings) -> Arc<Registry> {
    Arc::new(Registry::new(store, settings))
}

pub struct TestDevice {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub seed: [u8; 32],
    pub keys: DevicePublicKeys,
}

impl TestDevice {
    pub fn new(user_id: Uuid) -> Self {
        // The seed is a `Zeroizing<[u8; 32]>`; copying into the plain array is acceptable in
        // test code (there is no persisted ciphertext here to protect against leaking).
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
}

pub async fn register(registry: &Registry, device: &TestDevice) {
    registry
        .register_device(
            device.user_id,
            device.device_id,
            &device.keys,
            &device.proof(),
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
