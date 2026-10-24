use std::fmt;

use chrono::{DateTime, Utc};
use common::identity::DevicePublicKeys;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkToken {
    pub user_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

impl fmt::Debug for LinkToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkToken")
            .field("user_id", &self.user_id)
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredDevice {
    pub device_id: Uuid,
    pub keys: DevicePublicKeys,
    pub registered_at: DateTime<Utc>,
}
