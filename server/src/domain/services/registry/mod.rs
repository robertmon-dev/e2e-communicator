mod keys;
mod scripts;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use redis::AsyncCommands;
use uuid::Uuid;

use common::identity::{self, DevicePublicKeys, SIGNATURE_LEN};

use crate::core::initializers::redis::MailboxStore;
use crate::core::initializers::settings::AuthSettings;
use crate::domain::dto::errors::RegistryError;

use keys::{
    SESSION_PREFIX, challenge_key, device_key, device_sessions_key, link_key, session_key,
    user_devices_key,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredDevice {
    pub device_id: Uuid,
    pub keys: DevicePublicKeys,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[must_use]
#[derive(Clone, PartialEq, Eq)]
pub struct Session {
    pub token: String,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("token", &"<redacted>")
            .field("user_id", &self.user_id)
            .field("device_id", &self.device_id)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

pub struct Registry {
    store: Arc<MailboxStore>,
    settings: AuthSettings,
}

impl Registry {
    pub fn new(store: Arc<MailboxStore>, settings: AuthSettings) -> Self {
        Self { store, settings }
    }

    pub fn settings(&self) -> &AuthSettings {
        &self.settings
    }

    pub async fn register_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
    ) -> Result<(), RegistryError> {
        verify_self_proof(user_id, device_id, keys, self_proof)?;

        let outcome: i64 = self
            .store
            .run(|mut conn| async move {
                scripts::REGISTER_DEVICE
                    .key(user_devices_key(user_id))
                    .key(device_key(user_id, device_id))
                    .arg(device_id.to_string())
                    .arg(&keys.signing_key[..])
                    .arg(&keys.agreement_key[..])
                    .arg(Utc::now().timestamp_millis())
                    .invoke_async(&mut conn)
                    .await
            })
            .await?;

        match outcome {
            1 => {
                tracing::info!(%user_id, %device_id, "first device registered");
                Ok(())
            }
            0 => {
                tracing::debug!(%user_id, %device_id, "device re-registered with identical keys");
                Ok(())
            }
            -2 => {
                tracing::error!(
                    %user_id,
                    %device_id,
                    "device listed for user but hash is missing or partial"
                );
                Err(corrupt("device listed for user but hash is missing"))
            }
            -3 => {
                tracing::debug!(
                    %user_id,
                    %device_id,
                    "registration rejected: device id registered with different keys"
                );
                Err(RegistryError::DeviceTaken)
            }
            _ => {
                tracing::debug!(%user_id, %device_id, "registration rejected: user id taken");
                Err(RegistryError::UserTaken)
            }
        }
    }

    pub async fn lookup_devices(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<RegisteredDevice>, RegistryError> {
        let ids: Vec<String> = self
            .store
            .run(|mut conn| async move { conn.smembers(user_devices_key(user_id)).await })
            .await?;

        let mut devices = Vec::with_capacity(ids.len());
        for raw in ids {
            let device_id = match Uuid::parse_str(&raw) {
                Ok(device_id) => device_id,
                Err(_) => {
                    tracing::warn!(%user_id, raw, "skipping device set member that is not a uuid");
                    continue;
                }
            };
            if let Some(device) = self.read_device(user_id, device_id).await? {
                devices.push(device);
            }
        }

        devices.sort_by_key(|device| (device.registered_at, device.device_id));
        Ok(devices)
    }

    pub async fn create_link_token(&self, user_id: Uuid) -> Result<LinkToken, RegistryError> {
        let token = identity::encode_token(&identity::random_bytes::<32>()?);
        let ttl = self.settings.link_token_ttl_secs;
        let key = link_key(&token);

        self.store
            .run(|mut conn| async move {
                conn.set_ex::<_, _, ()>(&key, user_id.to_string(), ttl)
                    .await
            })
            .await?;

        tracing::info!(%user_id, "link token issued");

        Ok(LinkToken {
            token,
            expires_at: ttl_from_now(ttl),
        })
    }

    pub async fn link_device(
        &self,
        token: &str,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
    ) -> Result<Uuid, RegistryError> {
        let key = link_key(token);

        let user_id_raw: Option<String> = self
            .store
            .run(|mut conn| {
                let key = key.clone();
                async move { conn.get(key).await }
            })
            .await?;

        let user_id = match user_id_raw {
            Some(raw) => {
                Uuid::parse_str(&raw).map_err(|_| corrupt("link token does not point at a uuid"))?
            }
            None => {
                tracing::debug!(%device_id, "link rejected: unknown or already-consumed token");
                return Err(RegistryError::LinkTokenInvalid);
            }
        };

        verify_self_proof(user_id, device_id, keys, self_proof)?;

        let outcome: i64 = self
            .store
            .run(|mut conn| async move {
                scripts::LINK_DEVICE
                    .key(key)
                    .key(user_devices_key(user_id))
                    .key(device_key(user_id, device_id))
                    .arg(device_id.to_string())
                    .arg(&keys.signing_key[..])
                    .arg(&keys.agreement_key[..])
                    .arg(Utc::now().timestamp_millis())
                    .arg(user_id.to_string())
                    .invoke_async(&mut conn)
                    .await
            })
            .await?;

        match outcome {
            1 => {
                tracing::info!(%user_id, %device_id, "device linked");
                Ok(user_id)
            }
            0 => {
                tracing::debug!(%user_id, %device_id, "device re-linked with identical keys");
                Ok(user_id)
            }
            -1 => {
                tracing::debug!(
                    %user_id,
                    %device_id,
                    "link rejected: device id already registered with different keys"
                );
                Err(RegistryError::DeviceTaken)
            }
            -2 => {
                tracing::error!(
                    %user_id,
                    %device_id,
                    "device hash is only partially written"
                );
                Err(corrupt("device hash is only partially written"))
            }
            _ => {
                tracing::debug!(%user_id, %device_id, "link rejected: token already consumed");
                Err(RegistryError::LinkTokenInvalid)
            }
        }
    }

    pub async fn revoke_device(&self, user_id: Uuid, device_id: Uuid) -> Result<(), RegistryError> {
        let sessions_deleted: i64 = self
            .store
            .run(|mut conn| async move {
                scripts::REVOKE_DEVICE
                    .key(device_sessions_key(user_id, device_id))
                    .key(device_key(user_id, device_id))
                    .key(user_devices_key(user_id))
                    .arg(device_id.to_string())
                    .arg(SESSION_PREFIX)
                    .invoke_async(&mut conn)
                    .await
            })
            .await?;

        tracing::info!(%user_id, %device_id, sessions = sessions_deleted, "device revoked");
        Ok(())
    }

    pub async fn issue_challenge(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<[u8; 32], RegistryError> {
        if self.read_device(user_id, device_id).await?.is_none() {
            return Err(RegistryError::UnknownDevice);
        }

        let nonce = identity::random_bytes::<32>()?;
        let ttl = self.settings.challenge_ttl_secs;

        self.store
            .run(|mut conn| async move {
                conn.set_ex::<_, _, ()>(challenge_key(user_id, device_id), &nonce[..], ttl)
                    .await
            })
            .await?;

        tracing::debug!(%user_id, %device_id, "challenge issued");
        Ok(nonce)
    }

    pub async fn authenticate(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        signature: &[u8; SIGNATURE_LEN],
    ) -> Result<Session, RegistryError> {
        let nonce: Option<Vec<u8>> = self
            .store
            .run(|mut conn| async move {
                redis::cmd("GETDEL")
                    .arg(challenge_key(user_id, device_id))
                    .query_async(&mut conn)
                    .await
            })
            .await?;

        let nonce: [u8; 32] = match nonce {
            Some(bytes) => bytes
                .try_into()
                .map_err(|_| corrupt("stored nonce has wrong length"))?,
            None => {
                tracing::debug!(%user_id, %device_id, "authenticate rejected: no pending challenge");
                return Err(RegistryError::ChallengeMissing);
            }
        };

        let device = self
            .read_device(user_id, device_id)
            .await?
            .ok_or(RegistryError::UnknownDevice)?;

        let message = identity::canonical_challenge_bytes(user_id, device_id, &nonce);
        if identity::verify(&device.keys.signing_key, &message, signature).is_err() {
            tracing::debug!(%user_id, %device_id, "authenticate rejected: invalid signature");
            return Err(RegistryError::InvalidSignature);
        }

        let token = identity::encode_token(&identity::random_bytes::<32>()?);
        let ttl = self.settings.session_ttl_secs;
        let expires_at = ttl_from_now(ttl);
        let key = session_key(&token);
        let sessions_key = device_sessions_key(user_id, device_id);

        let written: i64 = self
            .store
            .run(|mut conn| {
                let token = token.as_str();
                async move {
                    scripts::CREATE_SESSION
                        .key(device_key(user_id, device_id))
                        .key(key)
                        .key(sessions_key)
                        .arg(user_id.to_string())
                        .arg(device_id.to_string())
                        .arg(expires_at.timestamp_millis())
                        .arg(ttl)
                        .arg(token)
                        .invoke_async(&mut conn)
                        .await
                }
            })
            .await?;

        if written == 0 {
            tracing::debug!(%user_id, %device_id, "authenticate rejected: device revoked mid-flight");
            return Err(RegistryError::UnknownDevice);
        }

        tracing::info!(%user_id, %device_id, "session created");

        Ok(Session {
            token,
            user_id,
            device_id,
            expires_at,
        })
    }

    pub async fn session(&self, token: &str) -> Result<Option<Session>, RegistryError> {
        let key = session_key(token);

        let map: HashMap<String, String> = self
            .store
            .run(|mut conn| async move { conn.hgetall(key).await })
            .await?;

        if map.is_empty() {
            return Ok(None);
        }

        let field = |name: &str| {
            map.get(name)
                .ok_or_else(|| corrupt(format!("session field '{name}' missing")))
        };
        let user_id = Uuid::parse_str(field("user_id")?)
            .map_err(|_| corrupt("session user_id is not a uuid"))?;
        let device_id = Uuid::parse_str(field("device_id")?)
            .map_err(|_| corrupt("session device_id is not a uuid"))?;
        let expires_at_ms: i64 = field("expires_at")?
            .parse()
            .map_err(|_| corrupt("session expires_at malformed"))?;
        let expires_at = DateTime::from_timestamp_millis(expires_at_ms)
            .ok_or_else(|| corrupt("session expires_at out of range"))?;

        Ok(Some(Session {
            token: token.to_string(),
            user_id,
            device_id,
            expires_at,
        }))
    }

    async fn read_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<Option<RegisteredDevice>, RegistryError> {
        let map: HashMap<String, Vec<u8>> = self
            .store
            .run(|mut conn| async move { conn.hgetall(device_key(user_id, device_id)).await })
            .await?;

        if map.is_empty() {
            return Ok(None);
        }

        let signing_key = map
            .get("signing_key")
            .ok_or_else(|| corrupt("signing_key missing"))?;
        let agreement_key = map
            .get("agreement_key")
            .ok_or_else(|| corrupt("agreement_key missing"))?;
        let registered_at_ms: i64 = map
            .get("registered_at")
            .and_then(|raw| std::str::from_utf8(raw).ok())
            .and_then(|raw| raw.parse().ok())
            .ok_or_else(|| corrupt("registered_at missing or malformed"))?;

        Ok(Some(RegisteredDevice {
            device_id,
            keys: DevicePublicKeys::from_slices(signing_key, agreement_key)
                .map_err(|_| corrupt("stored key has wrong length"))?,
            registered_at: DateTime::from_timestamp_millis(registered_at_ms)
                .ok_or_else(|| corrupt("registered_at out of range"))?,
        }))
    }
}

fn verify_self_proof(
    user_id: Uuid,
    device_id: Uuid,
    keys: &DevicePublicKeys,
    self_proof: &[u8; SIGNATURE_LEN],
) -> Result<(), RegistryError> {
    let message = identity::canonical_device_bytes(user_id, device_id, keys);
    identity::verify(&keys.signing_key, &message, self_proof)
        .map_err(|_| RegistryError::InvalidProof)
}

fn corrupt(reason: impl Into<String>) -> RegistryError {
    RegistryError::Corrupt(reason.into())
}

fn ttl_from_now(seconds: u64) -> DateTime<Utc> {
    let target = Utc::now() + Duration::seconds(seconds as i64);
    DateTime::from_timestamp_millis(target.timestamp_millis()).unwrap_or(target)
}
