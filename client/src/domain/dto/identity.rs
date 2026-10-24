use chrono::{DateTime, Utc};
use common::identity::DevicePublicKeys;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkToken {
    pub user_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredDevice {
    pub device_id: Uuid,
    pub keys: DevicePublicKeys,
    pub registered_at: DateTime<Utc>,
}
