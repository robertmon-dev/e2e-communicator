use ring::error::Unspecified;
use sea_orm::DbErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("invalid key material")]
    InvalidKey,
    #[error("Not implemented yet")]
    NotImplemented,
    #[error(transparent)]
    Persistence(#[from] DbErr),
}

#[derive(Debug, Error)]
pub enum RingEncryptorError {
    #[error(transparent)]
    Ring(#[from] Unspecified),
}

impl From<RingEncryptorError> for CryptoError {
    fn from(_: RingEncryptorError) -> Self {
        CryptoError::EncryptionFailed
    }
}
