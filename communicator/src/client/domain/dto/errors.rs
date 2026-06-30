use thiserror::Error;
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
pub enum ChatError {
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
