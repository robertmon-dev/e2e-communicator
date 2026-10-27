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

    #[error("Data is corrupted")]
    CorruptedData,

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
pub enum IdentityStoreError {
    #[error("a local identity already exists")]
    AlreadyExists,

    #[error(transparent)]
    Crypto(#[from] crate::domain::dto::error::CryptoError),

    #[error(transparent)]
    Persistence(#[from] DbErr),
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("identity is in the wrong state for this operation")]
    WrongState,

    #[error("no active session")]
    NoSession,

    #[error("user id is already claimed by another device")]
    UserTaken,

    #[error(transparent)]
    Transport(#[from] TransportError),

    #[error(transparent)]
    IdentityStore(#[from] IdentityStoreError),

    #[error(transparent)]
    Crypto(#[from] crate::domain::dto::error::CryptoError),

    #[error(transparent)]
    Persistence(#[from] DbErr),
}

#[derive(Debug, Error)]
pub enum ExchangeError {
    #[error("Failed to generate a local keypair")]
    KeyGenerationFailed,

    #[error("Public key with length {0} is not a valid X25519 key")]
    InvalidPublicKey(usize),

    #[error("No local private key stored for chat {0} / user {1} / device {2}")]
    MissingLocalPrivateKey(Uuid, Uuid, Uuid),

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

    #[error("Failed to receive envelope: {0}")]
    ReceiveFailed(String),

    #[error("Not authenticated: {0}")]
    Unauthenticated(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Error)]
pub enum InboxError {
    #[error(transparent)]
    Exchange(#[from] ExchangeError),

    #[error(transparent)]
    Crypto(#[from] CryptoError),

    #[error(transparent)]
    DeviceKeys(#[from] crate::domain::dto::error::CryptoError),

    #[error("Failed to serialize delivery receipt")]
    Serialization(#[from] serde_json::Error),

    #[error(transparent)]
    Persistence(#[from] DbErr),

    #[error(transparent)]
    PeerDirectory(#[from] PeerDirectoryError),

    #[error(transparent)]
    Invite(#[from] InviteError),

    #[error("unknown sender device {0}")]
    UnknownSenderDevice(Uuid),

    #[error("key bundle mismatch for chat {0}")]
    MismatchedKeyBundle(Uuid),
}

#[derive(Debug, Error)]
pub enum PeerDirectoryError {
    #[error("peer {0} identity does not match the contact code")]
    IdentityMismatch(Uuid),

    #[error("peer {0} identity key changed since it was pinned")]
    IdentityChanged(Uuid),

    #[error("peer {0} has no devices with a valid certificate")]
    NoVerifiedDevices(Uuid),

    #[error(transparent)]
    Transport(#[from] TransportError),

    #[error(transparent)]
    Database(#[from] DbErr),

    #[error("local identity is not registered")]
    Unregistered,
}

#[derive(Debug, Error)]
pub enum InviteError {
    #[error("contact code is malformed")]
    InvalidContactCode,

    #[error("chat key from {0}/{1} does not verify against its device key")]
    BrokenTrustChain(Uuid, Uuid),

    #[error("a direct chat or pending invitation with {0} already exists")]
    AlreadyInvited(Uuid),

    #[error("invitation {0} has already been resolved")]
    WrongState(Uuid),

    #[error("invitation {0} was not found")]
    NotFound(Uuid),

    #[error("missing permission to manage members of chat {0}")]
    Forbidden(Uuid),

    #[error(transparent)]
    PeerDirectory(#[from] PeerDirectoryError),

    #[error(transparent)]
    Exchange(#[from] ExchangeError),

    #[error(transparent)]
    Crypto(#[from] CryptoError),

    #[error(transparent)]
    DeviceKeys(#[from] crate::domain::dto::error::CryptoError),

    #[error(transparent)]
    Chat(#[from] ChatError),

    #[error(transparent)]
    Value(#[from] ValueError),

    #[error(transparent)]
    Database(#[from] DbErr),

    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("The vault is already unlocked")]
    AlreadyUnlocked,

    #[error("{0}")]
    BadPinFormat(String),

    #[error("The database could not be opened with that PIN")]
    WrongPin,

    #[error("Too many failed attempts; restart the client to try again")]
    TooManyAttempts,

    #[error("Unlocked, but starting the client failed: {0}")]
    Boot(String),
}
