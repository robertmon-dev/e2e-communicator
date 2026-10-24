use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use chrono::{DateTime, Duration, Utc};
use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use uuid::Uuid;

use common::identity::{self, DevicePublicKeys, SIGNATURE_LEN};

use crate::core::initializers::redis::MailboxStore;
use crate::core::initializers::settings::AuthSettings;
use crate::domain::dto::errors::RegistryError;

static REGISTER_DEVICE_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
local n = redis.call('SCARD', KEYS[1])
if n == 0 then
  redis.call('HSET', KEYS[2], 'signing_key', ARGV[2], 'agreement_key', ARGV[3], 'registered_at', ARGV[4])
  redis.call('SADD', KEYS[1], ARGV[1])
  return 1
end
if n == 1 and redis.call('SISMEMBER', KEYS[1], ARGV[1]) == 1 then
  local s = redis.call('HGET', KEYS[2], 'signing_key')
  local a = redis.call('HGET', KEYS[2], 'agreement_key')
  if s == ARGV[2] and a == ARGV[3] then return 0 end
  if not s and not a then
    redis.call('HSET', KEYS[2], 'signing_key', ARGV[2], 'agreement_key', ARGV[3], 'registered_at', ARGV[4])
    return 1
  end
  return -1
end
return -1
"#,
    )
});

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub token: String,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub expires_at: DateTime<Utc>,
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

        let mut conn = self.store.connection().await?;

        let outcome: i64 = REGISTER_DEVICE_SCRIPT
            .key(user_devices_key(user_id))
            .key(device_key(user_id, device_id))
            .arg(device_id.to_string())
            .arg(&keys.signing_key[..])
            .arg(&keys.agreement_key[..])
            .arg(Utc::now().timestamp_millis())
            .invoke_async(&mut conn)
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
            _ => Err(RegistryError::UserTaken),
        }
    }

    pub async fn lookup_devices(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<RegisteredDevice>, RegistryError> {
        let mut conn = self.store.connection().await?;
        let ids: Vec<String> = conn.smembers(user_devices_key(user_id)).await?;

        let mut devices = Vec::with_capacity(ids.len());
        for raw in ids {
            let device_id = match Uuid::parse_str(&raw) {
                Ok(device_id) => device_id,
                Err(_) => {
                    tracing::warn!(%user_id, raw, "skipping device set member that is not a uuid");
                    continue;
                }
            };
            if let Some(device) = read_device(&mut conn, user_id, device_id).await? {
                devices.push(device);
            }
        }

        devices.sort_by_key(|device| (device.registered_at, device.device_id));
        Ok(devices)
    }

    pub async fn create_link_token(&self, user_id: Uuid) -> Result<LinkToken, RegistryError> {
        let token = identity::encode_token(&identity::random_bytes::<32>()?);
        let ttl = self.settings.link_token_ttl_secs;

        let mut conn = self.store.connection().await?;
        let _: () = conn
            .set_ex(link_key(&token), user_id.to_string(), ttl)
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
        let mut conn = self.store.connection().await?;

        let user_id_raw: Option<String> = conn.get(link_key(token)).await?;
        let user_id = Uuid::parse_str(&user_id_raw.ok_or(RegistryError::LinkTokenInvalid)?)
            .map_err(|_| corrupt("link token does not point at a uuid"))?;

        verify_self_proof(user_id, device_id, keys, self_proof)?;

        let consumed: Option<String> = redis::cmd("GETDEL")
            .arg(link_key(token))
            .query_async(&mut conn)
            .await?;
        if consumed.is_none() {
            return Err(RegistryError::LinkTokenInvalid);
        }

        write_device(&mut conn, user_id, device_id, keys).await?;
        tracing::info!(%user_id, %device_id, "device linked");
        Ok(user_id)
    }

    pub async fn revoke_device(&self, user_id: Uuid, device_id: Uuid) -> Result<(), RegistryError> {
        let mut conn = self.store.connection().await?;
        let tokens: Vec<String> = conn
            .smembers(device_sessions_key(user_id, device_id))
            .await?;

        let mut pipe = redis::pipe();
        pipe.atomic();
        for token in &tokens {
            pipe.del(session_key(token)).ignore();
        }
        pipe.del(device_sessions_key(user_id, device_id)).ignore();
        pipe.del(device_key(user_id, device_id)).ignore();
        pipe.srem(user_devices_key(user_id), device_id.to_string())
            .ignore();
        let _: () = pipe.query_async(&mut conn).await?;

        tracing::info!(%user_id, %device_id, sessions = tokens.len(), "device revoked");
        Ok(())
    }

    pub async fn issue_challenge(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<[u8; 32], RegistryError> {
        let mut conn = self.store.connection().await?;
        if read_device(&mut conn, user_id, device_id).await?.is_none() {
            return Err(RegistryError::UnknownDevice);
        }

        let nonce = identity::random_bytes::<32>()?;
        let _: () = conn
            .set_ex(
                challenge_key(user_id, device_id),
                &nonce[..],
                self.settings.challenge_ttl_secs,
            )
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
        let mut conn = self.store.connection().await?;

        let nonce: Option<Vec<u8>> = redis::cmd("GETDEL")
            .arg(challenge_key(user_id, device_id))
            .query_async(&mut conn)
            .await?;
        let nonce: [u8; 32] = nonce
            .ok_or(RegistryError::ChallengeMissing)?
            .try_into()
            .map_err(|_| corrupt("stored nonce has wrong length"))?;

        let device = read_device(&mut conn, user_id, device_id)
            .await?
            .ok_or(RegistryError::UnknownDevice)?;

        let message = identity::canonical_challenge_bytes(user_id, device_id, &nonce);
        identity::verify(&device.keys.signing_key, &message, signature)
            .map_err(|_| RegistryError::InvalidSignature)?;

        let token = identity::encode_token(&identity::random_bytes::<32>()?);
        let ttl = self.settings.session_ttl_secs;
        let expires_at = ttl_from_now(ttl);
        let key = session_key(&token);

        let _: () = redis::pipe()
            .atomic()
            .hset(&key, "user_id", user_id.to_string())
            .ignore()
            .hset(&key, "device_id", device_id.to_string())
            .ignore()
            .hset(&key, "expires_at", expires_at.timestamp_millis())
            .ignore()
            .expire(&key, ttl as i64)
            .ignore()
            .sadd(device_sessions_key(user_id, device_id), &token)
            .ignore()
            .query_async(&mut conn)
            .await?;

        tracing::info!(%user_id, %device_id, "session created");

        Ok(Session {
            token,
            user_id,
            device_id,
            expires_at,
        })
    }

    pub async fn session(&self, token: &str) -> Result<Option<Session>, RegistryError> {
        let mut conn = self.store.connection().await?;
        let map: HashMap<String, String> = conn.hgetall(session_key(token)).await?;
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

async fn write_device(
    conn: &mut MultiplexedConnection,
    user_id: Uuid,
    device_id: Uuid,
    keys: &DevicePublicKeys,
) -> Result<(), RegistryError> {
    let key = device_key(user_id, device_id);
    let _: () = redis::pipe()
        .atomic()
        .hset(&key, "signing_key", &keys.signing_key[..])
        .ignore()
        .hset(&key, "agreement_key", &keys.agreement_key[..])
        .ignore()
        .hset(&key, "registered_at", Utc::now().timestamp_millis())
        .ignore()
        .sadd(user_devices_key(user_id), device_id.to_string())
        .ignore()
        .query_async(conn)
        .await?;
    Ok(())
}

async fn read_device(
    conn: &mut MultiplexedConnection,
    user_id: Uuid,
    device_id: Uuid,
) -> Result<Option<RegisteredDevice>, RegistryError> {
    let map: HashMap<String, Vec<u8>> = conn.hgetall(device_key(user_id, device_id)).await?;
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

fn corrupt(reason: impl Into<String>) -> RegistryError {
    RegistryError::Corrupt(reason.into())
}

fn user_devices_key(user_id: Uuid) -> String {
    format!("identity:user:{user_id}")
}

fn device_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("identity:device:{user_id}:{device_id}")
}

fn link_key(token: &str) -> String {
    format!("link:{token}")
}

fn challenge_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("challenge:{user_id}:{device_id}")
}

fn session_key(token: &str) -> String {
    format!("session:{token}")
}

fn device_sessions_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("sessions:{user_id}:{device_id}")
}

fn ttl_from_now(seconds: u64) -> DateTime<Utc> {
    let target = Utc::now() + Duration::seconds(seconds as i64);
    DateTime::from_timestamp_millis(target.timestamp_millis()).unwrap_or(target)
}
