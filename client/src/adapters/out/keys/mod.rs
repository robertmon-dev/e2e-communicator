use std::sync::Arc;

use async_trait::async_trait;
use orion::hazardous::ecc::x25519::{PrivateKey as X25519PrivateKey, PublicKey as X25519PublicKey};
use uuid::Uuid;
use zeroize::Zeroizing;

use common::identity::{self, DevicePublicKeys, KEY_LEN, SIGNATURE_LEN};

use crate::adapters::out::persistance::entities::device_key;
use crate::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use crate::domain::dto::error::CryptoError;
use crate::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};

pub struct DeviceKeyStore {
    device_key_repository: Arc<DeviceKeyRepository>,
    encryptor: Encryptor,
}

#[async_trait]
pub trait DeviceKeyStoreRepository: Send + Sync {
    async fn get_or_create_keys(&self, device_id: Uuid) -> Result<DevicePublicKeys, CryptoError>;

    async fn sign(
        &self,
        device_id: Uuid,
        message: &[u8],
    ) -> Result<[u8; SIGNATURE_LEN], CryptoError>;
}

impl DeviceKeyStore {
    pub fn new(device_key_repository: Arc<DeviceKeyRepository>, encryptor: Encryptor) -> Self {
        Self {
            device_key_repository,
            encryptor,
        }
    }
}

fn keys_from_model(model: &device_key::Model) -> Result<DevicePublicKeys, CryptoError> {
    DevicePublicKeys::from_slices(&model.signing_public_key, &model.public_key)
        .map_err(|_| CryptoError::InvalidKey)
}

#[async_trait]
impl DeviceKeyStoreRepository for DeviceKeyStore {
    async fn get_or_create_keys(&self, device_id: Uuid) -> Result<DevicePublicKeys, CryptoError> {
        if let Some(existing) = self.device_key_repository.get(device_id).await? {
            return keys_from_model(&existing);
        }

        let agreement_private = X25519PrivateKey::generate();
        let agreement_public =
            X25519PublicKey::try_from(&agreement_private).map_err(|_| CryptoError::InvalidKey)?;

        let signing_seed: Zeroizing<[u8; KEY_LEN]> =
            identity::generate_signing_seed().map_err(|_| CryptoError::InvalidKey)?;
        let signing_public = identity::signing_public_key(&signing_seed);

        let encrypted_agreement = self
            .encryptor
            .encrypt(agreement_private.unprotected_as_bytes())
            .map_err(|_| CryptoError::EncryptionFailed)?;
        let encrypted_signing = self
            .encryptor
            .encrypt(signing_seed.as_slice())
            .map_err(|_| CryptoError::EncryptionFailed)?;

        let created = self
            .device_key_repository
            .create(
                device_id,
                agreement_public.to_bytes().to_vec(),
                encrypted_agreement,
                signing_public.to_vec(),
                encrypted_signing,
            )
            .await?;

        keys_from_model(&created)
    }

    async fn sign(
        &self,
        device_id: Uuid,
        message: &[u8],
    ) -> Result<[u8; SIGNATURE_LEN], CryptoError> {
        let model = self
            .device_key_repository
            .get(device_id)
            .await?
            .ok_or(CryptoError::DeviceNotFound)?;

        let seed_bytes: Zeroizing<Vec<u8>> = Zeroizing::new(
            self.encryptor
                .decrypt(&model.encrypted_signing_private_key)
                .map_err(|_| CryptoError::DecryptionFailed)?,
        );
        let seed: Zeroizing<[u8; KEY_LEN]> = Zeroizing::new(
            seed_bytes
                .as_slice()
                .try_into()
                .map_err(|_| CryptoError::InvalidKey)?,
        );

        Ok(identity::sign(&seed, message))
    }
}
