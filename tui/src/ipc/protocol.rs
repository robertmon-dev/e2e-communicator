use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 2;
pub const SESSION_INIT_METHOD: &str = "session.init";

#[derive(Clone, Serialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub params: Value,
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params: &dyn fmt::Debug = if self.method == SESSION_INIT_METHOD {
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

#[derive(Debug, Deserialize)]
pub struct Response {
    pub id: Value,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Outcome {
    Ok { result: Value },
    Err { error: RpcError },
}

#[derive(Debug, Clone, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct Notification {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug)]
pub enum Frame {
    Response(Response),
    Notification(Notification),
}

pub fn parse_frame(line: &str) -> Result<Frame, serde_json::Error> {
    let value: Value = serde_json::from_str(line)?;

    if value.get("id").is_some() {
        serde_json::from_value(value).map(Frame::Response)
    } else {
        serde_json::from_value(value).map(Frame::Notification)
    }
}

pub fn short(id: &Uuid) -> String {
    id.simple().to_string()[..8].to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionInit {
    pub protocol_version: u32,
    pub identity_state: String,
    pub user_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub connectivity: String,
    pub authenticated: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct IdentityInfo {
    pub state: String,
    pub user_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
}

impl IdentityInfo {
    pub fn short_label(&self) -> Option<String> {
        self.user_id
            .as_ref()
            .map(|id| format!("user-{}", short(id)))
    }

    pub fn is_registered(&self) -> bool {
        self.state == "registered"
    }

    pub fn is_conflicted(&self) -> bool {
        self.state == "conflict"
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceKeysView {
    pub signing_key: String,
    pub agreement_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceView {
    pub device_id: Uuid,
    pub keys: DeviceKeysView,
    pub registered_at: DateTime<Utc>,
}

impl DeviceView {
    pub fn short_id(&self) -> String {
        short(&self.device_id)
    }
}

#[derive(Clone, Deserialize)]
pub struct LinkTokenView {
    pub user_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

impl LinkTokenView {
    pub fn qr_payload(&self) -> String {
        serde_json::json!({ "user_id": self.user_id, "token": self.token }).to_string()
    }
}

impl fmt::Debug for LinkTokenView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkTokenView")
            .field("user_id", &self.user_id)
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatStateView {
    pub name: Option<String>,
    #[serde(default)]
    pub members: BTreeMap<Uuid, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatView {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub history: BTreeMap<DateTime<Utc>, ChatStateView>,
}

impl ChatView {
    fn latest(&self) -> Option<&ChatStateView> {
        self.history.values().next_back()
    }

    pub fn title(&self) -> String {
        match self.latest().and_then(|state| state.name.as_deref()) {
            Some(name) if !name.trim().is_empty() => name.trim().to_string(),
            _ => crate::i18n::tf("ui.chat_fallback_title", &[("id", &short(&self.id))]),
        }
    }

    pub fn member_count(&self) -> usize {
        self.latest().map(|state| state.members.len()).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageContentView {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageView {
    pub id: Uuid,
    pub author_id: Uuid,
    pub chat_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub history: BTreeMap<DateTime<Utc>, MessageContentView>,
}

impl MessageView {
    pub fn text(&self) -> &str {
        self.history
            .values()
            .next_back()
            .map(|content| content.text.as_str())
            .unwrap_or("")
    }
}
