use thiserror::Error;

#[derive(Debug, Error)]
pub enum MailboxError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("invalid redis configuration: {0}")]
    InvalidConfig(String),

    #[error("stream entry is missing field '{0}'")]
    MissingField(String),

    #[error("stream entry has invalid field '{field}': {reason}")]
    InvalidField { field: String, reason: String },
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error(transparent)]
    Store(#[from] MailboxError),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("user id is already registered to a different device")]
    UserTaken,

    #[error("device id is already registered with different keys")]
    DeviceTaken,

    #[error("device self-proof is invalid")]
    InvalidProof,

    #[error("challenge signature is invalid")]
    InvalidSignature,

    #[error("no pending challenge for this device")]
    ChallengeMissing,

    #[error("device is not registered")]
    UnknownDevice,

    #[error("link token is unknown, used, or expired")]
    LinkTokenInvalid,

    #[error("registry data is corrupt: {0}")]
    Corrupt(String),

    #[error(transparent)]
    Identity(#[from] common::identity::IdentityError),
}
