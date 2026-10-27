use std::sync::Arc;

use serde_json::Value;

use common::identity::{encode_contact_code, encode_token, user_fingerprint};

use crate::adapters::r#in::ipc::facade::handlers::execute_rpc;
use crate::adapters::r#in::ipc::facade::params::{
    AttachmentDeleteManyParams, AttachmentDeleteParams, AttachmentGetParams, AttachmentListParams,
    ChatsCreateGroupParams, ChatsDeleteHistoryRecord, ChatsDeleteManyParams, ChatsGetParams,
    ChatsListParams, DevicesRevokeParams, IdentityLinkParams, InvitationsAcceptParams,
    InvitationsCancelParams, InvitationsCreateParams, InvitationsListParams,
    InvitationsRejectParams, MessagesDeleteParams, MessagesGetParams, MessagesListParams,
    MessagesMarkDeliveredParams, SendMessageParams, SessionInitParams, SessionInitResponse,
    UsersDeleteParams, UsersGetParams, UsersListParams, UsersSafetyNumberParams, UsersUpdateParams,
    UsersVerifyParams,
};
use crate::adapters::r#in::ipc::facade::utils::{
    CODE_WRONG_STATE, IntoOutcome, ok, parse, rpc_error,
};
use crate::adapters::r#in::ipc::protocol::identity_json;
use crate::adapters::out::keys::{UserIdentityStore, UserIdentityStoreRepository};
use crate::adapters::out::persistance::repositories::invitation::InvitationRepository;
use crate::adapters::out::persistance::repositories::peer_device::PeerDeviceRepository;
use crate::adapters::out::persistance::repositories::user::UserRepository;
use crate::adapters::out::persistance::repositories::{
    attachment::AttachmentRepository, chat::ChatRepository, message::MessageRepository,
};
use crate::domain::dto::local_identity::LocalIdentity;
use crate::domain::services::authenticator::Authenticator;
use crate::domain::services::inviter::Inviter;
use crate::domain::services::messenger::Messenger;
use crate::domain::services::peer_directory::PeerDirectory;
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

pub struct FacadeDeps {
    pub messenger: Arc<Messenger>,
    pub inviter: Arc<Inviter>,
    pub peer_directory: Arc<PeerDirectory>,
    pub chat_repository: Arc<ChatRepository>,
    pub message_repository: Arc<MessageRepository>,
    pub attachment_repository: Arc<AttachmentRepository>,
    pub user_repository: Arc<UserRepository>,
    pub invitation_repository: Arc<InvitationRepository>,
    pub peer_device_repository: Arc<PeerDeviceRepository>,
    pub user_identity: Arc<UserIdentityStore>,
    pub connectivity: Arc<ConnectivityState>,
    pub identity: Arc<IdentityState>,
    pub session: Arc<SessionState>,
    pub authenticator: Arc<Authenticator>,
    pub session_token: Option<String>,
}

pub struct Facade {
    pub(super) messenger: Arc<Messenger>,
    pub(super) inviter: Arc<Inviter>,
    pub(super) peer_directory: Arc<PeerDirectory>,
    pub(super) chat_repository: Arc<ChatRepository>,
    pub(super) message_repository: Arc<MessageRepository>,
    pub(super) attachment_repository: Arc<AttachmentRepository>,
    pub(super) user_repository: Arc<UserRepository>,
    pub(super) invitation_repository: Arc<InvitationRepository>,
    pub(super) peer_device_repository: Arc<PeerDeviceRepository>,
    pub(super) user_identity: Arc<UserIdentityStore>,
    pub(super) connectivity: Arc<ConnectivityState>,
    pub(super) identity: Arc<IdentityState>,
    pub(super) session: Arc<SessionState>,
    pub(super) authenticator: Arc<Authenticator>,
    pub(super) session_token: Option<String>,
}

impl Facade {
    pub fn new(deps: FacadeDeps) -> Self {
        Self {
            messenger: deps.messenger,
            inviter: deps.inviter,
            peer_directory: deps.peer_directory,
            chat_repository: deps.chat_repository,
            message_repository: deps.message_repository,
            attachment_repository: deps.attachment_repository,
            user_repository: deps.user_repository,
            invitation_repository: deps.invitation_repository,
            peer_device_repository: deps.peer_device_repository,
            user_identity: deps.user_identity,
            connectivity: deps.connectivity,
            identity: deps.identity,
            session: deps.session,
            authenticator: deps.authenticator,
            session_token: deps.session_token,
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
            "users.safety_number" => execute_rpc::<UsersSafetyNumberParams>(self, params).await,
            "users.verify" => execute_rpc::<UsersVerifyParams>(self, params).await,

            "invitations.contact_code" => self.invitations_contact_code().await,
            "invitations.create" => execute_rpc::<InvitationsCreateParams>(self, params).await,
            "invitations.list" => execute_rpc::<InvitationsListParams>(self, params).await,
            "invitations.accept" => execute_rpc::<InvitationsAcceptParams>(self, params).await,
            "invitations.reject" => execute_rpc::<InvitationsRejectParams>(self, params).await,
            "invitations.cancel" => execute_rpc::<InvitationsCancelParams>(self, params).await,

            "chats.create_group" => execute_rpc::<ChatsCreateGroupParams>(self, params).await,

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

    async fn invitations_contact_code(&self) -> Outcome {
        let identity = match self.require_identity() {
            Ok(identity) => identity,
            Err(error) => return Outcome::Err { error },
        };

        let key = match self.user_identity.get_or_create(identity.user_id).await {
            Ok(key) => key,
            Err(e) => {
                tracing::warn!("facade: failed to load own identity key: {e}");
                return rpc_error(-32000, "internal error");
            }
        };

        let fingerprint = user_fingerprint(&key);

        ok(serde_json::json!({
            "user_id": identity.user_id,
            "fingerprint": encode_token(&fingerprint),
            "code": encode_contact_code(identity.user_id, &fingerprint),
        }))
    }
}
