use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use crate::adapters::r#in::ipc::facade::params::{
    AttachmentDeleteManyParams, AttachmentDeleteParams, AttachmentGetParams, AttachmentListParams,
    ChatsGetParams, ChatsListParams, MessagesDeleteParams, MessagesGetParams, MessagesListParams,
    MessagesMarkDeliveredParams, SendMessageParams, SessionInitParams, SessionInitResponse,
};
use crate::adapters::r#in::ipc::facade::utils::{call_rpc, ok, parse};
use crate::adapters::out::persistance::repositories::attachment::remove::Remove;
use crate::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::adapters::out::persistance::repositories::chat::utils::ChatRestrictions;
use crate::adapters::out::persistance::repositories::{
    attachment::AttachmentRepository, message::MessageRepository,
};
use crate::domain::services::messenger::Messenger;
use crate::domain::state::connectivity::ConnectivityState;

pub mod handlers;
pub mod params;
pub mod utils;

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

impl Facade {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        messenger: Arc<Messenger>,
        chat_repository: Arc<ChatRepository>,
        message_repository: Arc<MessageRepository>,
        attachment_repository: Arc<AttachmentRepository>,
        connectivity: Arc<ConnectivityState>,
        user_id: Uuid,
        device_id: Uuid,
        session_token: Option<String>,
    ) -> Self {
        Self {
            messenger,
            chat_repository,
            message_repository,
            attachment_repository,
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
