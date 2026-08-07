use sea_orm::DbErr;
use thiserror::Error;
use url::ParseError;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Message content cannot be empty")]
    EmptyContent,

    #[error("Message content is too long. Maximum length is {max_len}, provided {actual_len}")]
    MessageTooLong { max_len: usize, actual_len: usize },

    #[error("Message is in an invalid state: missing change history")]
    EmptyHistory,

    #[error("User with ID {0} is not authorized to modify this message")]
    UnauthorizedAction(Uuid),

    #[error("Reaction with ID {0} was not found in this message")]
    ReactionNotFound(Uuid),

    #[error("User with ID {0} has already added a reaction to this message")]
    DuplicateReaction(Uuid),
}

#[derive(Debug, Error)]
pub enum ReactionError {
    #[error("Reaction content cannot be empty")]
    EmptyContent,

    #[error("Reaction content is too long. Maximum length is {max_len}, provided {actual_len}")]
    ReactionTooLong { max_len: usize, actual_len: usize },

    #[error("Reaction is in an invalid state: missing change history")]
    EmptyHistory,

    #[error("User with ID {0} is not authorized to modify this Reaction")]
    UnauthorizedAction(Uuid),

    #[error("Message with ID {0} was not found in this Reaction")]
    MessageNotFound(Uuid),
}

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("Chat with ID {0} not found")]
    ChatHistoryNotFound(Uuid),

    #[error("Chat with ID {0} not found")]
    ChatNotFound(Uuid),

    #[error("User with ID {0} is not a member of this chat")]
    UserNotMember(Uuid),

    #[error("User with ID {0} is already a member of this chat")]
    UserAlreadyMember(Uuid),

    #[error("Chat name cannot be empty")]
    EmptyChatName,

    #[error("Chat name is too long. Maximum length is {max_len}, provided {actual_len}")]
    ChatNameTooLong { max_len: usize, actual_len: usize },

    #[error("Cannot change the type of a Direct chat to {target_type:?}")]
    InvalidChatTypeTransition { target_type: String },

    #[error("Permission denied: {0}")]
    AccessDenied(String),
}

#[derive(Debug, Error)]
pub enum ValueError {
    #[error("Passed value is lesser than lower scope")]
    LesserThanExpected,

    #[error("Passed value is greater than upper scope")]
    GreaterThanExpected,

    #[error("Invalid URL format: {0}")]
    UrlParse(#[from] ParseError),

    #[error(transparent)]
    InvalidEventType(#[from] common::envelope::InvalidEventType),
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Failed to seal (encrypt) payload")]
    SealFailed,

    #[error(
        "Failed to open (decrypt) payload — invalid key, corrupted ciphertext, or tampering detected"
    )]
    OpenFailed,
}

#[derive(Debug, Error)]
pub enum ExchangeError {
    #[error("Failed to generate a local keypair")]
    KeyGenerationFailed,

    #[error("Public key with length {0} is not a valid X25519 key")]
    InvalidPublicKey(usize),

    #[error("No local private key stored for chat {0} / user {1}")]
    MissingLocalPrivateKey(Uuid, Uuid),

    #[error("Key agreement (ECDH) failed")]
    AgreementFailed,

    #[error(transparent)]
    Crypto(#[from] CryptoError),

    #[error(transparent)]
    Persistence(#[from] DbErr),
}

#[derive(Debug, Error)]
pub enum MessengerError {
    #[error(transparent)]
    Message(#[from] MessageError),

    #[error(transparent)]
    Chat(#[from] ChatError),

    #[error(transparent)]
    Exchange(#[from] ExchangeError),

    #[error(transparent)]
    Crypto(#[from] CryptoError),

    #[error("Failed to serialize message content")]
    Serialization(#[from] serde_json::Error),

    #[error(transparent)]
    Persistence(#[from] DbErr),
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Failed to send envelope: {0}")]
    SendFailed(String),
}
