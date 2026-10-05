use sea_orm::DbErr;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    adapters::r#in::ipc::protocol::{Outcome, RpcError},
    domain::dto::errors::{MessageError, MessengerError},
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
