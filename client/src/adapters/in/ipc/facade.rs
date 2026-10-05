use std::sync::Arc;

use chrono::{DateTime, Utc};
use sea_orm::DbErr;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use crate::adapters::out::persistance::repositories::attachment::remove::Remove;
use crate::adapters::out::persistance::repositories::attachment::utils::AttachmentRestrictions;
use crate::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::adapters::out::persistance::repositories::chat::utils::ChatRestrictions;
use crate::adapters::out::persistance::repositories::message::MessageRepository;
use crate::domain::dto::errors::{MessageError, MessengerError};
use crate::domain::dto::filter_attachments::ListAttachmentsFilter;
use crate::domain::dto::filter_chat::ListChatsFilter;
use crate::domain::dto::filter_messages::ListMessagesFilter;
use crate::domain::dto::message::{Attachment, MessageContent};
use crate::domain::dto::utils::{PaginationMeta, TimeStep};
use crate::domain::services::messenger::Messenger;
use crate::domain::state::connectivity::ConnectivityState;

use super::protocol::{Outcome, Request, Response, RpcError};

pub const SUPPORTED_PROTOCOL_VERSIONS: &[u32] = &[1, 2];

pub struct Facade {
    messenger: Arc<Messenger>,
    chat_repository: Arc<ChatRepository>,
    message_repository: Arc<MessageRepository>,
    attachment_repository: Arc<AttachmentRepository>,
    connectivity: Arc<ConnectivityState>,
    user_id: Uuid,
    device_id: Uuid,
    session_token: Option<String>,
}

#[derive(Deserialize)]
pub struct SessionInitParams {
    pub protocol_version: u32,
    pub token: Option<String>,
}

#[derive(Serialize)]
pub struct SessionInitResponse {
    pub protocol_version: u32,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub connectivity: String,
}

fn db_error(err: DbErr) -> Outcome {
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

fn ok(result: Value) -> Outcome {
    Outcome::Ok { result }
}

fn parse<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, Outcome> {
    serde_json::from_value(params).map_err(|e| Outcome::Err {
        error: RpcError {
            code: -32602,
            message: format!("invalid params: {e}"),
        },
    })
}

fn messenger_error(err: MessengerError) -> Outcome {
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

trait IntoOutcome {
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

impl<T: Serialize> IntoOutcome for Result<T, MessengerError> {
    fn into_outcome(self) -> Outcome {
        match self {
            Ok(val) => ok(serde_json::to_value(val).expect("Serialization failed")),
            Err(e) => messenger_error(e),
        }
    }
}

async fn call_rpc<P, F, Fut, R>(params: Value, handler: F) -> Outcome
where
    P: for<'de> Deserialize<'de>,
    F: FnOnce(P) -> Fut,
    Fut: std::future::Future<Output = R>,
    R: IntoOutcome,
{
    let parsed: P = match parse(params) {
        Ok(p) => p,
        Err(e) => return e,
    };
    handler(parsed).await.into_outcome()
}

impl Facade {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        messenger: Arc<Messenger>,
        chat_repository: Arc<ChatRepository>,
        attachment_repository: Arc<AttachmentRepository>,
        message_repository: Arc<MessageRepository>,
        connectivity: Arc<ConnectivityState>,
        user_id: Uuid,
        device_id: Uuid,
        session_token: Option<String>,
    ) -> Self {
        Self {
            messenger,
            chat_repository,
            attachment_repository,
            message_repository,
            connectivity,
            user_id,
            device_id,
            session_token,
        }
    }

    pub fn authenticate_session(&self, params: Value) -> Result<SessionInitResponse, RpcError> {
        let params: SessionInitParams = parse(params).map_err(|e| match e {
            Outcome::Err { error } => error,
            _ => unreachable!(),
        })?;

        if !SUPPORTED_PROTOCOL_VERSIONS.contains(&params.protocol_version) {
            return Err(RpcError {
                code: -32003,
                message: format!("unsupported protocol version: {}", params.protocol_version),
            });
        }

        if let Some(ref expected_token) = self.session_token {
            match params.token {
                Some(ref provided) if provided == expected_token => {}
                _ => {
                    return Err(RpcError {
                        code: -32004,
                        message: "unauthorized: invalid or missing session token".to_string(),
                    });
                }
            }
        }

        Ok(SessionInitResponse {
            protocol_version: params.protocol_version,
            user_id: self.user_id,
            device_id: self.device_id,
            connectivity: format!("{:?}", self.connectivity.status()),
        })
    }

    pub async fn handle(&self, request: Request) -> Response {
        let outcome = self.dispatch(&request.method, request.params).await;
        Response {
            id: request.id,
            outcome,
        }
    }

    async fn dispatch(&self, method: &str, params: Value) -> Outcome {
        match method {
            "identity.get" => self.identity_get(),
            "connectivity.get_status" => self.connectivity_get_status(),

            "chats.list" => {
                let restrictions = ChatRestrictions {
                    restricted_chat_ids: None,
                    restricted_user_ids: None,
                    historical_memberships: None,
                    only_active: Some(true),
                };
                call_rpc(params, |p: ChatsListParams| {
                    self.chat_repository
                        .list(p.filter, p.pagination, restrictions)
                })
                .await
            }
            "chats.get" => {
                call_rpc(params, |p: ChatsGetParams| {
                    self.chat_repository.get(p.chat_id)
                })
                .await
            }

            "attachments.get" => {
                call_rpc(params, |p: AttachmentGetParams| {
                    self.attachment_repository.get_attachment(p.attachment_id)
                })
                .await
            }
            "attachments.lists" => {
                call_rpc(params, |p: AttachmentListParams| {
                    self.attachment_repository
                        .list(p.filters, p.pagination, p.restrictions)
                })
                .await
            }
            "attachments.delete" => {
                call_rpc(params, |p: AttachmentDeleteParams| {
                    self.attachment_repository
                        .remove_attachments(vec![p.attachment_id])
                })
                .await
            }
            "attachments.delete_many" => {
                call_rpc(params, |p: AttachmentDeleteManyParams| {
                    self.attachment_repository
                        .remove_attachments(p.attachment_ids)
                })
                .await
            }

            "messages.list" => {
                call_rpc(params, |p: MessagesListParams| {
                    self.message_repository
                        .list(p.filter, p.pagination, None, None, None, false)
                })
                .await
            }
            "messages.get" => {
                call_rpc(params, |p: MessagesGetParams| {
                    self.message_repository.get(p.message_id)
                })
                .await
            }
            "messages.send" => {
                call_rpc(params, |p: SendMessageParams| async move {
                    self.messenger
                        .send(
                            p.chat_id,
                            self.user_id,
                            p.text,
                            p.reply_to_id,
                            p.available_for,
                            p.attachments,
                            p.scheduled_for,
                        )
                        .await
                        .map(|(message, _envelopes)| message)
                })
                .await
            }
            "messages.mark_delivered" => {
                call_rpc(params, |p: MessagesMarkDeliveredParams| async move {
                    self.messenger
                        .mark_delivered(p.message_id, self.user_id)
                        .await
                        .map(|_| serde_json::json!({ "success": true }))
                })
                .await
            }
            "messages.delete" => {
                call_rpc(params, |p: MessagesDeleteParams| async move {
                    self.messenger
                        .delete(p.message_id, self.user_id)
                        .await
                        .map(|_| serde_json::json!({ "success": true }))
                })
                .await
            }

            other => Outcome::Err {
                error: RpcError {
                    code: -32601,
                    message: format!("method not found: {other}"),
                },
            },
        }
    }

    fn identity_get(&self) -> Outcome {
        ok(serde_json::json!({ "user_id": self.user_id, "device_id": self.device_id }))
    }

    fn connectivity_get_status(&self) -> Outcome {
        ok(serde_json::to_value(self.connectivity.status())
            .expect("ConnectivityStatus always serializes"))
    }
}

#[derive(Deserialize)]
struct ChatsListParams {
    filter: ListChatsFilter,
    pagination: PaginationMeta,
}

#[derive(Deserialize)]
struct ChatsGetParams {
    chat_id: Uuid,
}

#[derive(Deserialize)]
struct MessagesListParams {
    filter: ListMessagesFilter,
    pagination: PaginationMeta,
}

#[derive(Deserialize)]
struct SendMessageParams {
    chat_id: Uuid,
    text: String,
    reply_to_id: Option<Uuid>,
    available_for: Option<TimeStep>,
    #[serde(default)]
    attachments: Vec<Attachment>,
    scheduled_for: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct MessagesGetParams {
    message_id: Uuid,
}

#[derive(Deserialize)]
struct MessagesDeleteParams {
    message_id: Uuid,
}

#[derive(Deserialize)]
struct MessagesMarkDeliveredParams {
    message_id: Uuid,
}

#[derive(Deserialize)]
struct AttachmentGetParams {
    attachment_id: Uuid,
}

#[derive(Deserialize)]
struct AttachmentListParams {
    filters: ListAttachmentsFilter,
    pagination: PaginationMeta,
    restrictions: AttachmentRestrictions,
}

#[derive(Deserialize)]
struct AttachmentDeleteParams {
    attachment_id: Uuid,
}

#[derive(Deserialize)]
struct AttachmentDeleteManyParams {
    attachment_ids: Vec<Uuid>,
}

#[derive(Deserialize)]
struct MessageUpdateParams {
    message_id: Uuid,
    message_history_record: MessageContent,
}
