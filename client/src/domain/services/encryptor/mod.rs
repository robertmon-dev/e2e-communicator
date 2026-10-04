use orion::aead::{SecretKey, open, seal};

use crate::domain::dto::errors::CryptoError;

pub trait PrivateKeyEncryptor: Send + Sync {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError>;
    fn encrypt_with_derived_key(key: &SecretKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError>
    where
        Self: Sized;
    fn decrypt_with_derived_key(key: &SecretKey, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError>
    where
        Self: Sized;
}

pub struct Encryptor {
    key: SecretKey,
}

impl Encryptor {
    pub fn new(key: SecretKey) -> Self {
        Self { key }
    }
}

impl PrivateKeyEncryptor for Encryptor {
    fn encrypt_with_derived_key(key: &SecretKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        seal(key, plaintext).map_err(|_| CryptoError::SealFailed)
    }

    fn decrypt_with_derived_key(
        key: &SecretKey,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        open(key, ciphertext).map_err(|_| CryptoError::OpenFailed)
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        seal(&self.key, plaintext).map_err(|_| CryptoError::SealFailed)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        open(&self.key, ciphertext).map_err(|_| CryptoError::OpenFailed)
    }
}
