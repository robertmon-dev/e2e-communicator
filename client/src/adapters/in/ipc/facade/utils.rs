use sea_orm::DbErr;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    adapters::r#in::ipc::protocol::{Outcome, RpcError},
    domain::dto::errors::{
        AuthError, IdentityStoreError, InviteError, MessageError, MessengerError,
        PeerDirectoryError, TransportError,
    },
};

pub fn db_error(err: DbErr) -> Outcome {
    match &err {
        DbErr::RecordNotFound(_) => Outcome::Err {
            error: RpcError {
                code: -32002,
                message: "not found".to_string(),
            },
        },
        other => {
            tracing::warn!("facade: db error: {other}");
            Outcome::Err {
                error: RpcError {
                    code: -32000,
                    message: "internal error".to_string(),
                },
            }
        }
    }
}

pub fn ok(result: Value) -> Outcome {
    Outcome::Ok { result }
}

pub fn parse<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, Outcome> {
    serde_json::from_value(params).map_err(|e| Outcome::Err {
        error: RpcError {
            code: -32602,
            message: format!("invalid params: {e}"),
        },
    })
}

pub fn messenger_error(err: MessengerError) -> Outcome {
    match err {
        MessengerError::Message(MessageError::EmptyContent) => Outcome::Err {
            error: RpcError {
                code: -32001,
                message: "message content cannot be empty".to_string(),
            },
        },
        MessengerError::Persistence(db_err) => db_error(db_err),
        other => {
            tracing::warn!("facade: messenger error: {other}");
            Outcome::Err {
                error: RpcError {
                    code: -32000,
                    message: "internal error".to_string(),
                },
            }
        }
    }
}

pub trait IntoOutcome {
    fn into_outcome(self) -> Outcome;
}

impl<T: Serialize> IntoOutcome for Result<T, DbErr> {
    fn into_outcome(self) -> Outcome {
        match self {
            Ok(val) => ok(serde_json::to_value(val).expect("Serialization failed")),
            Err(e) => db_error(e),
        }
    }
}

impl IntoOutcome for DbErr {
    fn into_outcome(self) -> Outcome {
        db_error(self)
    }
}

impl<T: Serialize> IntoOutcome for Result<T, MessengerError> {
    fn into_outcome(self) -> Outcome {
        match self {
            Ok(val) => ok(serde_json::to_value(val).expect("Serialization failed")),
            Err(e) => messenger_error(e),
        }
    }
}

impl IntoOutcome for MessengerError {
    fn into_outcome(self) -> Outcome {
        messenger_error(self)
    }
}

pub const CODE_WRONG_STATE: i32 = -32005;
pub const CODE_LINK_REJECTED: i32 = -32007;
pub const CODE_NOT_AUTHENTICATED: i32 = -32008;
pub const CODE_IDENTITY_MISMATCH: i32 = -32012;
pub const CODE_INVITE_REJECTED: i32 = -32011;

pub fn rpc_error(code: i32, message: impl Into<String>) -> Outcome {
    Outcome::Err {
        error: RpcError {
            code,
            message: message.into(),
        },
    }
}

pub fn auth_error(err: AuthError) -> Outcome {
    match err {
        AuthError::WrongState => rpc_error(CODE_WRONG_STATE, "identity is in the wrong state"),
        AuthError::NoSession => {
            rpc_error(CODE_NOT_AUTHENTICATED, "not authenticated with the relay")
        }
        AuthError::UserTaken => rpc_error(CODE_LINK_REJECTED, "that id is already taken"),
        AuthError::Transport(
            TransportError::Unauthenticated(_) | TransportError::PermissionDenied(_),
        ) => rpc_error(CODE_NOT_AUTHENTICATED, "the relay rejected the session"),
        AuthError::Transport(TransportError::NotFound(_) | TransportError::AlreadyExists(_)) => {
            rpc_error(CODE_LINK_REJECTED, "the relay rejected the link")
        }
        AuthError::IdentityStore(IdentityStoreError::AlreadyExists) => {
            rpc_error(CODE_WRONG_STATE, "a local identity already exists")
        }
        AuthError::IdentityStore(IdentityStoreError::Persistence(db_err))
        | AuthError::Persistence(db_err) => db_error(db_err),
        other => {
            tracing::warn!("facade: auth error: {other}");
            rpc_error(-32000, "internal error")
        }
    }
}

pub enum FacadeError {
    Rpc(RpcError),
    Db(DbErr),
    Messenger(MessengerError),
    Auth(AuthError),
}

impl From<RpcError> for FacadeError {
    fn from(err: RpcError) -> Self {
        Self::Rpc(err)
    }
}

impl From<DbErr> for FacadeError {
    fn from(err: DbErr) -> Self {
        Self::Db(err)
    }
}

impl From<MessengerError> for FacadeError {
    fn from(err: MessengerError) -> Self {
        Self::Messenger(err)
    }
}

impl From<AuthError> for FacadeError {
    fn from(err: AuthError) -> Self {
        Self::Auth(err)
    }
}

impl IntoOutcome for FacadeError {
    fn into_outcome(self) -> Outcome {
        match self {
            FacadeError::Rpc(error) => Outcome::Err { error },
            FacadeError::Db(err) => db_error(err),
            FacadeError::Messenger(err) => messenger_error(err),
            FacadeError::Auth(err) => auth_error(err),
        }
    }
}

impl<T: Serialize> IntoOutcome for Result<T, FacadeError> {
    fn into_outcome(self) -> Outcome {
        match self {
            Ok(value) => ok(serde_json::to_value(value).expect("Serialization failed")),
            Err(err) => err.into_outcome(),
        }
    }
}

impl<T: Serialize> IntoOutcome for Result<T, AuthError> {
    fn into_outcome(self) -> Outcome {
        match self {
            Ok(value) => ok(serde_json::to_value(value).expect("Serialization failed")),
            Err(err) => auth_error(err),
        }
    }
}

impl IntoOutcome for AuthError {
    fn into_outcome(self) -> Outcome {
        auth_error(self)
    }
}

pub fn peer_directory_error(err: PeerDirectoryError) -> Outcome {
    match err {
        PeerDirectoryError::IdentityMismatch(_) | PeerDirectoryError::IdentityChanged(_) => {
            rpc_error(CODE_IDENTITY_MISMATCH, err.to_string())
        }
        other => {
            tracing::warn!("facade: peer directory error: {other}");
            rpc_error(-32000, "internal error")
        }
    }
}

impl IntoOutcome for PeerDirectoryError {
    fn into_outcome(self) -> Outcome {
        peer_directory_error(self)
    }
}

impl<T: Serialize> IntoOutcome for Result<T, PeerDirectoryError> {
    fn into_outcome(self) -> Outcome {
        match self {
            Ok(value) => ok(serde_json::to_value(value).expect("Serialization failed")),
            Err(err) => peer_directory_error(err),
        }
    }
}

pub fn invite_error(err: InviteError) -> Outcome {
    match err {
        InviteError::WrongState(_) | InviteError::NotFound(_) => {
            rpc_error(CODE_WRONG_STATE, err.to_string())
        }
        InviteError::AlreadyInvited(_) | InviteError::Forbidden(_) => {
            rpc_error(CODE_INVITE_REJECTED, err.to_string())
        }
        InviteError::InvalidContactCode => rpc_error(-32602, err.to_string()),
        InviteError::PeerDirectory(peer_err) => peer_directory_error(peer_err),
        other => {
            tracing::warn!("facade: invite error: {other}");
            rpc_error(-32000, "internal error")
        }
    }
}

impl IntoOutcome for InviteError {
    fn into_outcome(self) -> Outcome {
        invite_error(self)
    }
}

impl<T: Serialize> IntoOutcome for Result<T, InviteError> {
    fn into_outcome(self) -> Outcome {
        match self {
            Ok(value) => ok(serde_json::to_value(value).expect("Serialization failed")),
            Err(err) => invite_error(err),
        }
    }
}
