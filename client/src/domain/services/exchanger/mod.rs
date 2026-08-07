use std::sync::Arc;

use chrono::Utc;
use orion::aead::SecretKey;
use orion::hazardous::ecc::x25519::{
    PrivateKey as X25519PrivateKey, PublicKey as X25519PublicKey, key_agreement,
};
use orion::hazardous::kdf::hkdf;
use uuid::Uuid;

use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::adapters::out::persistance::repositories::key::create::Create;
use crate::domain::dto::errors::ExchangeError;
use crate::domain::dto::key::Key;
use crate::domain::services::encryptor::Encryptor;

const SHARED_KEY_INFO: &[u8] = b"e2e-communicator/chat-shared-key";

pub struct Exchanger {
    key_repository: Arc<KeyRepository>,
    master_key: SecretKey,
}

impl Exchanger {
    pub fn new(key_repository: Arc<KeyRepository>, master_key: SecretKey) -> Self {
        Self {
            key_repository,
            master_key,
        }
    }

    pub async fn generate_local_key(
        &self,
        chat_id: Uuid,
        user_id: Uuid,
    ) -> Result<Key, ExchangeError> {
        let private_key = X25519PrivateKey::generate();
        let public_key = X25519PublicKey::try_from(&private_key)
            .map_err(|_| ExchangeError::KeyGenerationFailed)?;

        let encrypted_private_key =
            Encryptor::seal(&self.master_key, private_key.unprotected_as_bytes())?;

        let now = Utc::now();
        let key = Key::new(
            Uuid::new_v4(),
            chat_id,
            user_id,
            public_key.to_bytes().to_vec(),
            Some(encrypted_private_key),
            now,
            now,
        );

        self.key_repository.create(key.clone()).await?;

        Ok(key)
    }

    pub async fn store_peer_key(
        &self,
        chat_id: Uuid,
        peer_user_id: Uuid,
        public_key: Vec<u8>,
    ) -> Result<Key, ExchangeError> {
        X25519PublicKey::from_slice(&public_key)
            .map_err(|_| ExchangeError::InvalidPublicKey(public_key.len()))?;

        let now = Utc::now();
        let key = Key::new(
            Uuid::new_v4(),
            chat_id,
            peer_user_id,
            public_key,
            None,
            now,
            now,
        );

        self.key_repository.create(key.clone()).await?;

        Ok(key)
    }

    pub async fn derive_shared_key(
        &self,
        chat_id: Uuid,
        local_user_id: Uuid,
        peer_user_id: Uuid,
    ) -> Result<SecretKey, ExchangeError> {
        let local_key = self
            .key_repository
            .get_by_chat_and_user(chat_id, local_user_id)
            .await?;

        let encrypted_private_key =
            local_key
                .encrypted_private_key
                .ok_or(ExchangeError::MissingLocalPrivateKey(
                    chat_id,
                    local_user_id,
                ))?;

        let private_key_bytes = Encryptor::open(&self.master_key, &encrypted_private_key)?;
        let private_key = X25519PrivateKey::from_slice(&private_key_bytes)
            .map_err(|_| ExchangeError::AgreementFailed)?;

        let peer_key = self
            .key_repository
            .get_by_chat_and_user(chat_id, peer_user_id)
            .await?;

        let peer_public_key = X25519PublicKey::from_slice(&peer_key.public_key)
            .map_err(|_| ExchangeError::InvalidPublicKey(peer_key.public_key.len()))?;

        let shared_secret = key_agreement(&private_key, &peer_public_key)
            .map_err(|_| ExchangeError::AgreementFailed)?;

        let mut derived = [0u8; 32];
        hkdf::sha256::derive_key(
            chat_id.as_bytes(),
            shared_secret.unprotected_as_bytes(),
            Some(SHARED_KEY_INFO),
            &mut derived,
        )
        .map_err(|_| ExchangeError::AgreementFailed)?;

        SecretKey::from_slice(&derived).map_err(|_| ExchangeError::AgreementFailed)
    }
}
