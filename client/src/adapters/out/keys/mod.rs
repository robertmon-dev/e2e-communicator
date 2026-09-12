use std::sync::Arc;

use sea_orm_migration::async_trait::async_trait;
use uuid::Uuid;

use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::domain::dto::error::CryptoError;

pub struct DeviceKeyStore {
    key_repository: Arc<KeyRepository>,
}

#[async_trait]
pub trait DeviceKeyStoreRepository: Send + Sync {
    async fn get_or_create_keypair(&self, device_id: Uuid) -> Result<Vec<u8>, CryptoError>;
}

impl DeviceKeyStore {
    pub fn new(key_repository: Arc<KeyRepository>) -> Self {
        Self { key_repository }
    }
}

#[async_trait]
impl DeviceKeyStoreRepository for DeviceKeyStore {
    async fn get_or_create_keypair(&self, device_id: Uuid) -> Result<Vec<u8>, CryptoError> {
        Err(CryptoError::NotImplemented)
    }
}
