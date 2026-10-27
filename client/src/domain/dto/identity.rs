use std::fmt;

use chrono::{DateTime, Utc};
use common::identity::{DevicePublicKeys, KEY_LEN, SIGNATURE_LEN};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkToken {
    pub user_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    #[serde(with = "key_b64")]
    pub user_seed: [u8; KEY_LEN],
}

impl fmt::Debug for LinkToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkToken")
            .field("user_id", &self.user_id)
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("user_seed", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct IssuedLinkToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

impl fmt::Debug for IssuedLinkToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IssuedLinkToken")
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
    #[serde(with = "cert_b64")]
    pub cert: [u8; SIGNATURE_LEN],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserDirectory {
    #[serde(with = "key_b64")]
    pub user_signing_key: [u8; KEY_LEN],
    pub devices: Vec<RegisteredDevice>,
}

pub(crate) mod key_b64 {
    use common::identity::{KEY_LEN, decode_token_array, encode_token};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; KEY_LEN], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&encode_token(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; KEY_LEN], D::Error>
    where
        D: Deserializer<'de>,
    {
        let token = String::deserialize(deserializer)?;
        decode_token_array::<KEY_LEN>(&token).map_err(serde::de::Error::custom)
    }
}

pub(crate) mod cert_b64 {
    use common::identity::{SIGNATURE_LEN, decode_token_array, encode_token};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; SIGNATURE_LEN], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&encode_token(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; SIGNATURE_LEN], D::Error>
    where
        D: Deserializer<'de>,
    {
        let token = String::deserialize(deserializer)?;
        decode_token_array::<SIGNATURE_LEN>(&token).map_err(serde::de::Error::custom)
    }
}
