use std::sync::Arc;

use serde_json::Value;

use crate::adapters::r#in::ipc::facade::handlers::execute_rpc;
use crate::adapters::r#in::ipc::facade::params::{
    AttachmentDeleteManyParams, AttachmentDeleteParams, AttachmentGetParams, AttachmentListParams,
    ChatsDeleteHistoryRecord, ChatsDeleteManyParams, ChatsGetParams, ChatsListParams,
    DevicesRevokeParams, IdentityLinkParams, MessagesDeleteParams, MessagesGetParams,
    MessagesListParams, MessagesMarkDeliveredParams, SendMessageParams, SessionInitParams,
    SessionInitResponse, UsersDeleteParams, UsersGetParams, UsersListParams, UsersUpdateParams,
};
use crate::adapters::r#in::ipc::facade::utils::{CODE_WRONG_STATE, IntoOutcome, ok, parse};
use crate::adapters::r#in::ipc::protocol::identity_json;
use crate::adapters::out::persistance::repositories::user::UserRepository;
use crate::adapters::out::persistance::repositories::{
    attachment::AttachmentRepository, chat::ChatRepository, message::MessageRepository,
};
use crate::domain::dto::local_identity::LocalIdentity;
use crate::domain::services::authenticator::Authenticator;
use crate::domain::services::messenger::Messenger;
use crate::domain::state::connectivity::ConnectivityState;
use crate::domain::state::identity::IdentityState;
use crate::domain::state::session::SessionState;
use crate::domain::state::vault::VaultStatus;

pub mod handlers;
pub mod params;
pub mod utils;

use super::protocol::{Outcome, Request, Response, RpcError};

pub const SUPPORTED_PROTOCOL_VERSIONS: &[u32] = &[1, 2];

pub fn verify_handshake(
    params: Value,
    expected_token: Option<&str>,
) -> Result<SessionInitParams, RpcError> {
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

    if let Some(expected) = expected_token {
        match params.token {
            Some(ref provided) if provided == expected => {}
            _ => {
                return Err(RpcError {
                    code: -32004,
                    message: "unauthorized: invalid or missing session token".to_string(),
                });
            }
        }
    }

    Ok(params)
}

pub struct Facade {
    pub(super) messenger: Arc<Messenger>,
    pub(super) chat_repository: Arc<ChatRepository>,
    pub(super) message_repository: Arc<MessageRepository>,
    pub(super) attachment_repository: Arc<AttachmentRepository>,
    pub(super) user_repository: Arc<UserRepository>,
    pub(super) connectivity: Arc<ConnectivityState>,
    pub(super) identity: Arc<IdentityState>,
    pub(super) session: Arc<SessionState>,
    pub(super) authenticator: Arc<Authenticator>,
    pub(super) session_token: Option<String>,
}

impl Facade {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        messenger: Arc<Messenger>,
        chat_repository: Arc<ChatRepository>,
        user_repository: Arc<UserRepository>,
        message_repository: Arc<MessageRepository>,
        attachment_repository: Arc<AttachmentRepository>,
        connectivity: Arc<ConnectivityState>,
        identity: Arc<IdentityState>,
        session: Arc<SessionState>,
        authenticator: Arc<Authenticator>,
        session_token: Option<String>,
    ) -> Self {
        Self {
            messenger,
            chat_repository,
            message_repository,
            attachment_repository,
            user_repository,
            connectivity,
            identity,
            session,
            authenticator,
            session_token,
        }
    }

    pub(super) fn require_identity(&self) -> Result<LocalIdentity, RpcError> {
        self.identity.registered().ok_or_else(|| RpcError {
            code: CODE_WRONG_STATE,
            message: "identity is not registered".to_string(),
        })
    }

    pub fn authenticate_session(&self, params: Value) -> Result<SessionInitResponse, RpcError> {
        let params = verify_handshake(params, self.session_token.as_deref())?;

        let status = self.identity.current();
        let identity = status.identity();

        Ok(SessionInitResponse {
            protocol_version: params.protocol_version,
            vault_state: VaultStatus::Unlocked.label().to_string(),
            identity_state: status.label().to_string(),
            user_id: identity.map(|identity| identity.user_id),
            device_id: identity.map(|identity| identity.device_id),
            connectivity: format!("{:?}", self.connectivity.status()),
            authenticated: self.session.is_authenticated(),
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
            "identity.register" => self.identity_register().await,
            "identity.forget" => self.identity_forget().await,
            "identity.link" => execute_rpc::<IdentityLinkParams>(self, params).await,
            "connectivity.get_status" => self.connectivity_get_status(),

            "devices.create_link_token" => self.devices_create_link_token().await,
            "devices.list" => self.devices_list().await,
            "devices.revoke" => execute_rpc::<DevicesRevokeParams>(self, params).await,

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

            "users.list" => execute_rpc::<UsersListParams>(self, params).await,
            "users.get" => execute_rpc::<UsersGetParams>(self, params).await,
            "users.update" => execute_rpc::<UsersUpdateParams>(self, params).await,
            "users.delete" => execute_rpc::<UsersDeleteParams>(self, params).await,

            other => Outcome::Err {
                error: RpcError {
                    code: -32601,
                    message: format!("method not found: {other}"),
                },
            },
        }
    }

    fn identity_get(&self) -> Outcome {
        ok(identity_json(&self.identity.current()))
    }

    async fn identity_register(&self) -> Outcome {
        self.authenticator.register_new().await.into_outcome()
    }

    async fn identity_forget(&self) -> Outcome {
        match self.authenticator.forget_identity().await {
            Ok(()) => ok(serde_json::json!({})),
            Err(err) => err.into_outcome(),
        }
    }

    async fn devices_create_link_token(&self) -> Outcome {
        self.authenticator.create_link_token().await.into_outcome()
    }

    async fn devices_list(&self) -> Outcome {
        self.authenticator.list_devices().await.into_outcome()
    }

    fn connectivity_get_status(&self) -> Outcome {
        ok(serde_json::to_value(self.connectivity.status())
            .expect("ConnectivityStatus always serializes"))
    }
}
