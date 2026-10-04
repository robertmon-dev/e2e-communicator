use std::sync::Arc;

use orion::hazardous::ecc::x25519::{PrivateKey as X25519PrivateKey, PublicKey as X25519PublicKey};
use sea_orm_migration::async_trait::async_trait;
use uuid::Uuid;

use crate::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use crate::domain::dto::error::CryptoError;
use crate::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};

pub struct DeviceKeyStore {
    device_key_repository: Arc<DeviceKeyRepository>,
    encryptor: Encryptor,
}

#[async_trait]
pub trait DeviceKeyStoreRepository: Send + Sync {
    async fn get_or_create_keypair(&self, device_id: Uuid) -> Result<Vec<u8>, CryptoError>;
}

impl DeviceKeyStore {
    pub fn new(device_key_repository: Arc<DeviceKeyRepository>, encryptor: Encryptor) -> Self {
        Self {
            device_key_repository,
            encryptor,
        }
    }
}

#[async_trait]
impl DeviceKeyStoreRepository for DeviceKeyStore {
    async fn get_or_create_keypair(&self, device_id: Uuid) -> Result<Vec<u8>, CryptoError> {
        if let Some(existing) = self.device_key_repository.get(device_id).await? {
            return Ok(existing.public_key);
        }

        let private_key = X25519PrivateKey::generate();
        let public_key =
            X25519PublicKey::try_from(&private_key).map_err(|_| CryptoError::InvalidKey)?;

        let encrypted_private_key = self
            .encryptor
            .encrypt(private_key.unprotected_as_bytes())
            .map_err(|_| CryptoError::EncryptionFailed)?;

        let created = self
            .device_key_repository
            .create(
                device_id,
                public_key.to_bytes().to_vec(),
                encrypted_private_key,
            )
            .await?;

        Ok(created.public_key)
    }
}
