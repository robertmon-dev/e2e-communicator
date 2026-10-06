use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use crate::adapters::r#in::ipc::facade::handlers::execute_rpc;
use crate::adapters::r#in::ipc::facade::params::{
    AttachmentDeleteManyParams, AttachmentDeleteParams, AttachmentGetParams, AttachmentListParams,
    ChatsDeleteHistoryRecord, ChatsDeleteManyParams, ChatsGetParams, ChatsListParams,
    MessagesDeleteParams, MessagesGetParams, MessagesListParams, MessagesMarkDeliveredParams,
    SendMessageParams, SessionInitParams, SessionInitResponse,
};
use crate::adapters::r#in::ipc::facade::utils::{ok, parse};
use crate::adapters::out::persistance::repositories::{
    attachment::AttachmentRepository, chat::ChatRepository, message::MessageRepository,
};
use crate::domain::services::messenger::Messenger;
use crate::domain::state::connectivity::ConnectivityState;

pub mod handlers;
pub mod params;
pub mod utils;

use super::protocol::{Outcome, Request, Response, RpcError};

pub const SUPPORTED_PROTOCOL_VERSIONS: &[u32] = &[1, 2];

pub struct Facade {
    pub(super) messenger: Arc<Messenger>,
    pub(super) chat_repository: Arc<ChatRepository>,
    pub(super) message_repository: Arc<MessageRepository>,
    pub(super) attachment_repository: Arc<AttachmentRepository>,
    pub(super) connectivity: Arc<ConnectivityState>,
    pub(super) user_id: Uuid,
    pub(super) device_id: Uuid,
    pub(super) session_token: Option<String>,
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

            "chats.list" => execute_rpc::<ChatsListParams>(self, params).await,
            "chats.get" => execute_rpc::<ChatsGetParams>(self, params).await,
            "chats.delete_many" => execute_rpc::<ChatsDeleteManyParams>(self, params).await,
            "chats.delete_history_record" => {
                execute_rpc::<ChatsDeleteHistoryRecord>(self, params).await
            }

            "attachments.get" => execute_rpc::<AttachmentGetParams>(self, params).await,
            "attachments.lists" => execute_rpc::<AttachmentListParams>(self, params).await,
            "attachments.delete" => execute_rpc::<AttachmentDeleteParams>(self, params).await,
            "attachments.delete_many" => {
                execute_rpc::<AttachmentDeleteManyParams>(self, params).await
            }

            "messages.list" => execute_rpc::<MessagesListParams>(self, params).await,
            "messages.get" => execute_rpc::<MessagesGetParams>(self, params).await,
            "messages.send" => execute_rpc::<SendMessageParams>(self, params).await,
            "messages.delete" => execute_rpc::<MessagesDeleteParams>(self, params).await,
            "messages.mark_delivered" => {
                execute_rpc::<MessagesMarkDeliveredParams>(self, params).await
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
