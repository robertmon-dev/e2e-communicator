use std::sync::Arc;

use uuid::Uuid;

use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::domain::dto::error::CryptoError;
use crate::domain::services::encryptor::PrivateKeyEncryptor;

pub struct DeviceKeyStore {
    key_repository: Arc<KeyRepository>,
    encryptor: Arc<dyn PrivateKeyEncryptor>,
}

impl DeviceKeyStore {
    pub fn new(
        key_repository: Arc<KeyRepository>,
        encryptor: Arc<dyn PrivateKeyEncryptor>,
    ) -> Self {
        Self {
            key_repository,
            encryptor,
        }
    }

    pub async fn get_or_create_keypair(&self, device_id: Uuid) -> Result<Vec<u8>, CryptoError> {
        Err(CryptoError::NotImplemented)
    }
}
