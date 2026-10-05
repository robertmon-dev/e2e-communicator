use std::sync::Arc;

use chrono::{DateTime, Utc};
use sea_orm::DbErr;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::adapters::out::persistance::entities::attachment;
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
use crate::domain::dto::message::Attachment;
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

impl Facade {
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

            "chats.list" => self.chats_list(params).await,
            "chats.get" => self.chats_get(params).await,

            "attachments.get" => self.attachments_get(params).await,
            "attachments.lists" => self.attachments_list(params).await,
            "attachments.delete" => self.attachments_delete(params).await,

            "messages.list" => self.messages_list(params).await,
            "messages.send" => self.messages_send(params).await,
            "messages.get" => self.messages_get(params).await,
            "messages.delete" => self.messages_delete(params).await,
            "messages.mark_delivered" => self.messages_mark_delivered(params).await,
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

    async fn chats_list(&self, params: Value) -> Outcome {
        let params: ChatsListParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let restrictions = ChatRestrictions {
            restricted_chat_ids: None,
            restricted_user_ids: None,
            historical_memberships: None,
            only_active: Some(true),
        };
        match self
            .chat_repository
            .list(params.filter, params.pagination, restrictions)
            .await
        {
            Ok(chats) => ok(serde_json::to_value(chats).expect("Chat always serializes")),
            Err(e) => db_error(e),
        }
    }

    async fn attachments_delete(&self, params: Value) -> Outcome {
        let params: AttachmentDeleteParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        match self
            .attachment_repository
            .remove_attachment(params.attachment_id)
            .await
        {
            Ok(attachment) => {
                ok(serde_json::to_value(attachment).expect("Attachment always serializes"))
            }
            Err(e) => db_error(e),
        }
    }

    async fn attachments_get(&self, params: Value) -> Outcome {
        let params: AttachmentGetParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        match self
            .attachment_repository
            .get_attachment(params.attachment_id)
            .await
        {
            Ok(attachment) => {
                ok(serde_json::to_value(attachment).expect("Attachment always serializes"))
            }
            Err(e) => db_error(e),
        }
    }

    async fn attachments_list(&self, params: Value) -> Outcome {
        let params: AttachmentListParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        match self
            .attachment_repository
            .list(params.filters, params.pagination, params.restrictions)
            .await
        {
            Ok(attachments) => {
                ok(serde_json::to_value(attachments).expect("Attachments always serialize"))
            }
            Err(e) => db_error(e),
        }
    }

    async fn chats_get(&self, params: Value) -> Outcome {
        let params: ChatsGetParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };
        match self.chat_repository.get(params.chat_id).await {
            Ok(chat) => ok(serde_json::to_value(chat).expect("Chat always serializes")),
            Err(e) => db_error(e),
        }
    }

    async fn messages_list(&self, params: Value) -> Outcome {
        let params: MessagesListParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };
        match self
            .message_repository
            .list(params.filter, params.pagination, None, None, None, false)
            .await
        {
            Ok(messages) => ok(serde_json::to_value(messages).expect("Message always serializes")),
            Err(e) => db_error(e),
        }
    }

    async fn messages_get(&self, params: Value) -> Outcome {
        let params: MessagesGetParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        match self.message_repository.get(params.message_id).await {
            Ok(message) => ok(serde_json::to_value(message).expect("Message always serializes")),
            Err(e) => db_error(e),
        }
    }

    async fn messages_mark_delivered(&self, params: Value) -> Outcome {
        let params: MessagesMarkDeliveredParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        match self
            .messenger
            .mark_delivered(params.message_id, self.user_id)
            .await
        {
            Ok(_) => ok(serde_json::json!({ "success": true })),
            Err(e) => messenger_error(e),
        }
    }

    async fn messages_delete(&self, params: Value) -> Outcome {
        let params: MessagesDeleteParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        match self.messenger.delete(params.message_id, self.user_id).await {
            Ok(_) => ok(serde_json::json!({ "success": true })),
            Err(e) => messenger_error(e),
        }
    }

    async fn messages_send(&self, params: Value) -> Outcome {
        let params: SendMessageParams = match parse(params) {
            Ok(p) => p,
            Err(e) => return e,
        };
        match self
            .messenger
            .send(
                params.chat_id,
                self.user_id,
                params.text,
                params.reply_to_id,
                params.available_for,
                params.attachments,
                params.scheduled_for,
            )
            .await
        {
            Ok((message, _envelopes)) => {
                ok(serde_json::to_value(message).expect("Message always serializes"))
            }
            Err(e) => messenger_error(e),
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
