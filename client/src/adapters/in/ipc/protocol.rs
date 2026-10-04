use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::state::facade_events::FacadeEvent;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
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
        }
    }
}
