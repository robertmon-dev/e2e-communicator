use orion::aead::{SecretKey, open, seal};

use crate::domain::dto::errors::CryptoError;

pub trait PrivateKeyEncryptor: Send + Sync {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError>;
}

pub struct Encryptor {}

impl Encryptor {
    pub fn seal(key: &SecretKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        seal(key, plaintext).map_err(|_| CryptoError::SealFailed)
    }

    pub fn open(key: &SecretKey, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        open(key, ciphertext).map_err(|_| CryptoError::OpenFailed)
    }
}
