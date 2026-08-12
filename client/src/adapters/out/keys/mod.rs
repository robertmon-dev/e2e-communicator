use std::sync::Arc;
use thiserror::Error;

use uuid::Uuid;

use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::domain::dto::errors::CryptoError;

pub trait PrivateKeyEncryptor: Send + Sync {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError>;
}

#[derive(Debug, Error)]
pub enum DeviceKeyStoreError {
    #[error("Functionality is not yet implemented")]
    NotImplemented = 0,
}

pub struct DeviceKeyStore {
    key_repository: Arc<KeyRepository>,
    encryptor: Arc<dyn PrivateKeyEncryptor>,
}

impl DeviceKeyStore {
    pub async fn get_or_create_keypair(
        &self,
        device_id: Uuid,
    ) -> Result<Vec<u8>, DeviceKeyStoreError> {
        Err(DeviceKeyStoreError::NotImplemented)
    }
}
