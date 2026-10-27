use std::sync::Arc;

use async_trait::async_trait;
use orion::aead::SecretKey;
use orion::hazardous::ecc::x25519::{
    PrivateKey as X25519PrivateKey, PublicKey as X25519PublicKey, key_agreement,
};
use orion::hazardous::kdf::hkdf;
use uuid::Uuid;
use zeroize::Zeroizing;

use common::identity::{self, DevicePublicKeys, KEY_LEN, SIGNATURE_LEN};

use crate::adapters::out::persistance::entities::device_key;
use crate::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use crate::adapters::out::persistance::repositories::user_identity::UserIdentityRepository;
use crate::domain::dto::error::CryptoError;
use crate::domain::dto::errors::IdentityStoreError;
use crate::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};

const DEVICE_SEAL_INFO: &[u8] = b"e2e-communicator/device-seal/v1";

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

    async fn derive_device_shared_key(
        &self,
        device_id: Uuid,
        peer_device_id: Uuid,
        peer_agreement_key: &[u8; KEY_LEN],
    ) -> Result<SecretKey, CryptoError>;
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

    async fn derive_device_shared_key(
        &self,
        device_id: Uuid,
        peer_device_id: Uuid,
        peer_agreement_key: &[u8; KEY_LEN],
    ) -> Result<SecretKey, CryptoError> {
        let model = self
            .device_key_repository
            .get(device_id)
            .await?
            .ok_or(CryptoError::DeviceNotFound)?;

        let private_bytes: Zeroizing<Vec<u8>> = Zeroizing::new(
            self.encryptor
                .decrypt(&model.encrypted_private_key)
                .map_err(|_| CryptoError::DecryptionFailed)?,
        );
        let private_key = X25519PrivateKey::from_slice(private_bytes.as_slice())
            .map_err(|_| CryptoError::InvalidKey)?;
        let peer_public =
            X25519PublicKey::from_slice(peer_agreement_key).map_err(|_| CryptoError::InvalidKey)?;

        let shared =
            key_agreement(&private_key, &peer_public).map_err(|_| CryptoError::InvalidKey)?;

        let (first, second) = if device_id <= peer_device_id {
            (device_id, peer_device_id)
        } else {
            (peer_device_id, device_id)
        };
        let mut salt = Vec::with_capacity(32);
        salt.extend_from_slice(first.as_bytes());
        salt.extend_from_slice(second.as_bytes());

        let mut derived = Zeroizing::new([0u8; KEY_LEN]);
        hkdf::sha256::derive_key(
            &salt,
            shared.unprotected_as_bytes(),
            Some(DEVICE_SEAL_INFO),
            derived.as_mut_slice(),
        )
        .map_err(|_| CryptoError::InvalidKey)?;

        SecretKey::from_slice(derived.as_slice()).map_err(|_| CryptoError::InvalidKey)
    }
}

pub struct UserIdentityStore {
    user_identity_repository: Arc<UserIdentityRepository>,
    encryptor: Encryptor,
}

#[async_trait]
pub trait UserIdentityStoreRepository: Send + Sync {
    async fn get_or_create(&self, user_id: Uuid) -> Result<[u8; KEY_LEN], CryptoError>;

    async fn import_seed(
        &self,
        user_id: Uuid,
        seed: &[u8; KEY_LEN],
    ) -> Result<[u8; KEY_LEN], CryptoError>;

    async fn export_seed(&self) -> Result<Zeroizing<[u8; KEY_LEN]>, CryptoError>;

    async fn certify_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        device_keys: &DevicePublicKeys,
    ) -> Result<[u8; SIGNATURE_LEN], CryptoError>;
}

impl UserIdentityStore {
    pub fn new(
        user_identity_repository: Arc<UserIdentityRepository>,
        encryptor: Encryptor,
    ) -> Self {
        Self {
            user_identity_repository,
            encryptor,
        }
    }
}

#[async_trait]
impl UserIdentityStoreRepository for UserIdentityStore {
    async fn get_or_create(&self, user_id: Uuid) -> Result<[u8; KEY_LEN], CryptoError> {
        if let Some(existing) = self.user_identity_repository.get().await? {
            return identity::fixed(&existing.signing_public_key)
                .map_err(|_| CryptoError::InvalidKey);
        }

        let seed = identity::generate_signing_seed().map_err(|_| CryptoError::InvalidKey)?;
        self.import_seed(user_id, &seed).await
    }

    async fn import_seed(
        &self,
        user_id: Uuid,
        seed: &[u8; KEY_LEN],
    ) -> Result<[u8; KEY_LEN], CryptoError> {
        let public = identity::signing_public_key(seed);
        let encrypted = self
            .encryptor
            .encrypt(seed.as_slice())
            .map_err(|_| CryptoError::EncryptionFailed)?;

        self.user_identity_repository
            .create(user_id, public.to_vec(), encrypted)
            .await
            .map_err(|e| match e {
                IdentityStoreError::AlreadyExists => CryptoError::UserIdentityAlreadyExists,
                IdentityStoreError::Crypto(crypto) => crypto,
                IdentityStoreError::Persistence(db) => CryptoError::Persistence(db),
            })?;

        Ok(public)
    }

    async fn export_seed(&self) -> Result<Zeroizing<[u8; KEY_LEN]>, CryptoError> {
        let model = self
            .user_identity_repository
            .get()
            .await?
            .ok_or(CryptoError::UserIdentityNotFound)?;

        let bytes: Zeroizing<Vec<u8>> = Zeroizing::new(
            self.encryptor
                .decrypt(&model.encrypted_signing_private_key)
                .map_err(|_| CryptoError::DecryptionFailed)?,
        );

        Ok(Zeroizing::new(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| CryptoError::InvalidKey)?,
        ))
    }

    async fn certify_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        device_keys: &DevicePublicKeys,
    ) -> Result<[u8; SIGNATURE_LEN], CryptoError> {
        let seed = self.export_seed().await?;
        Ok(identity::sign(
            &seed,
            &identity::canonical_device_cert_bytes(user_id, device_id, device_keys),
        ))
    }
}
