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
