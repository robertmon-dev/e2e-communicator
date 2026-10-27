use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::state::facade_events::FacadeEvent;
use crate::domain::state::identity::IdentityStatus;

#[derive(Deserialize)]
pub struct Request {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

pub const VAULT_UNLOCK_METHOD: &str = "vault.unlock";

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: &dyn std::fmt::Debug = if self.method == VAULT_UNLOCK_METHOD {
            &"<redacted>"
        } else {
            &self.params
        };

        f.debug_struct("Request")
            .field("id", &self.id)
            .field("method", &self.method)
            .field("params", params)
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub id: Value,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Outcome {
    Ok { result: Value },
    Err { error: RpcError },
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Notification {
    pub method: String,
    pub params: Value,
}

impl From<FacadeEvent> for Notification {
    fn from(event: FacadeEvent) -> Self {
        match event {
            FacadeEvent::MessageReceived(message) => Notification {
                method: "message.received".to_string(),
                params: serde_json::to_value(message).expect("Message always serializes"),
            },
            FacadeEvent::DeliveryReceiptMarked {
                message_id,
                recipient_id,
            } => Notification {
                method: "message.delivered".to_string(),
                params: serde_json::json!({
                    "message_id": message_id,
                    "recipient_id": recipient_id,
                }),
            },
            FacadeEvent::ConnectivityChanged(status) => Notification {
                method: "connectivity.changed".to_string(),
                params: serde_json::to_value(status).expect("ConnectivityStatus always serializes"),
            },
            FacadeEvent::IdentityChanged(status) => Notification {
                method: "identity.changed".to_string(),
                params: identity_json(&status),
            },
            FacadeEvent::SessionChanged { authenticated } => Notification {
                method: "session.changed".to_string(),
                params: serde_json::json!({ "authenticated": authenticated }),
            },
            FacadeEvent::VaultChanged(status) => Notification {
                method: "vault.changed".to_string(),
                params: serde_json::json!({ "state": status.label() }),
            },
            FacadeEvent::InvitationReceived(invitation) => Notification {
                method: "invitation.received".to_string(),
                params: serde_json::to_value(invitation).expect("Invitation always serializes"),
            },
            FacadeEvent::InvitationResolved {
                invitation_id,
                status,
            } => Notification {
                method: "invitation.resolved".to_string(),
                params: serde_json::json!({
                    "invitation_id": invitation_id,
                    "status": status,
                }),
            },
            FacadeEvent::ChatActivated(chat_id) => Notification {
                method: "chat.activated".to_string(),
                params: serde_json::json!({ "chat_id": chat_id }),
            },
            FacadeEvent::MemberAdded { chat_id, user_id } => Notification {
                method: "chat.member_added".to_string(),
                params: serde_json::json!({
                    "chat_id": chat_id,
                    "user_id": user_id,
                }),
            },
            FacadeEvent::KeyExchangeProgress {
                chat_id,
                known,
                total,
            } => Notification {
                method: "chat.key_exchange_progress".to_string(),
                params: serde_json::json!({
                    "chat_id": chat_id,
                    "known": known,
                    "total": total,
                }),
            },
        }
    }
}

pub fn identity_json(status: &IdentityStatus) -> Value {
    let identity = status.identity();

    serde_json::json!({
        "state": status.label(),
        "user_id": identity.map(|identity| identity.user_id),
        "device_id": identity.map(|identity| identity.device_id),
    })
}
