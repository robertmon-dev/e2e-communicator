use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::Value;
use redis::aio::MultiplexedConnection;
use redis::streams::{StreamAutoClaimOptions, StreamId, StreamReadOptions, StreamReadReply};
use uuid::Uuid;

use common::envelope::{EncryptedEnvelope, EventType};

use crate::core::initializers::redis::MailboxStore;
use crate::domain::dto::errors::MailboxError;

const CONSUMER_GROUP: &str = "relay";
const AUTOCLAIM_MIN_IDLE_MS: usize = 0;
const READ_BLOCK_MS: usize = 5000;
const READ_COUNT: usize = 20;
const ONLINE_TTL_SECONDS: u64 = 20;
pub const PENDING_CHANNEL: &str = "mailbox:pending";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingNotification {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub envelope_id: Uuid,
}

impl PendingNotification {
    pub fn encode(&self) -> String {
        format!("{}:{}:{}", self.user_id, self.device_id, self.envelope_id)
    }

    pub fn decode(raw: &str) -> Result<Self, MailboxError> {
        let parts: Vec<&str> = raw.split(':').collect();
        let [user_id, device_id, envelope_id] = parts[..] else {
            return Err(MailboxError::InvalidField {
                field: "mailbox:pending payload".to_string(),
                reason: format!("expected 3 colon-separated fields, got {}", parts.len()),
            });
        };

        let parse_field_uuid = |value: &str, field: &str| {
            Uuid::parse_str(value).map_err(|_| MailboxError::InvalidField {
                field: field.to_string(),
                reason: "not a valid uuid".to_string(),
            })
        };

        Ok(Self {
            user_id: parse_field_uuid(user_id, "user_id")?,
            device_id: parse_field_uuid(device_id, "device_id")?,
            envelope_id: parse_field_uuid(envelope_id, "envelope_id")?,
        })
    }
}

pub struct Mailbox {
    store: Arc<MailboxStore>,
}

impl Mailbox {
    pub fn new(store: Arc<MailboxStore>) -> Self {
        Self { store }
    }

    pub async fn pubsub_connection(&self) -> Result<redis::aio::PubSub, MailboxError> {
        self.store.pubsub_connection().await
    }

    pub async fn register_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<(), MailboxError> {
        self.store
            .run(|mut conn| async move {
                let _: usize = conn
                    .sadd(devices_key(user_id), device_id.to_string())
                    .await?;

                let stream_key = mailbox_key(user_id, device_id);
                let created: redis::RedisResult<()> = conn
                    .xgroup_create_mkstream(&stream_key, CONSUMER_GROUP, "0")
                    .await;

                // The group already existing is the normal case on reconnect.
                match created {
                    Err(err) if err.code() != Some("BUSYGROUP") => Err(err),
                    _ => Ok(()),
                }
            })
            .await?;

        tracing::debug!(%user_id, %device_id, "device registered");

        Ok(())
    }

    pub async fn forget_device(&self, user_id: Uuid, device_id: Uuid) -> Result<(), MailboxError> {
        self.store
            .run(|mut conn| async move {
                redis::pipe()
                    .atomic()
                    .srem(devices_key(user_id), device_id.to_string())
                    .ignore()
                    .del(mailbox_key(user_id, device_id))
                    .ignore()
                    .del(push_token_key(user_id, device_id))
                    .ignore()
                    .del(online_key(user_id, device_id))
                    .ignore()
                    .query_async::<()>(&mut conn)
                    .await
            })
            .await?;

        tracing::info!(%user_id, %device_id, "device forgotten");

        Ok(())
    }

    pub async fn fan_out(&self, envelope: &EncryptedEnvelope) -> Result<(), MailboxError> {
        let items = envelope_items(envelope);
        let notify = envelope.notify;

        self.store
            .run(|mut conn| async move {
                let device_ids: Vec<String> = match envelope.recipient_device_id {
                    Some(device_id) => {
                        let known: bool = conn
                            .sismember(devices_key(envelope.recipient_id), device_id.to_string())
                            .await?;

                        if known {
                            vec![device_id.to_string()]
                        } else {
                            tracing::warn!(
                                envelope_id = %envelope.id,
                                recipient_id = %envelope.recipient_id,
                                %device_id,
                                "dropping envelope addressed to an unregistered device"
                            );
                            Vec::new()
                        }
                    }
                    None => conn.smembers(devices_key(envelope.recipient_id)).await?,
                };

                tracing::debug!(
                    envelope_id = %envelope.id,
                    recipient_id = %envelope.recipient_id,
                    device_count = device_ids.len(),
                    "fanning out envelope"
                );

                for device_id in device_ids {
                    let stream_key = format!("mailbox:{}:{}", envelope.recipient_id, device_id);
                    let _: Option<String> = conn.xadd(&stream_key, "*", &items).await?;

                    if !notify {
                        continue;
                    }

                    let Ok(device_uuid) = Uuid::parse_str(&device_id) else {
                        tracing::warn!(
                            %device_id,
                            "skipping pending notification for malformed device id"
                        );
                        continue;
                    };

                    let notification = PendingNotification {
                        user_id: envelope.recipient_id,
                        device_id: device_uuid,
                        envelope_id: envelope.id,
                    };

                    if let Err(err) = conn
                        .publish::<_, _, usize>(PENDING_CHANNEL, notification.encode())
                        .await
                    {
                        tracing::warn!(
                            error = %err,
                            %device_id,
                            "failed to publish mailbox:pending event"
                        );
                    }
                }

                Ok(())
            })
            .await
    }

    pub async fn blocking_connection(&self) -> Result<MultiplexedConnection, MailboxError> {
        self.store.blocking_connection().await
    }

    pub async fn read_next(
        &self,
        conn: &mut MultiplexedConnection,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<Vec<(String, EncryptedEnvelope)>, MailboxError> {
        let stream_key = mailbox_key(user_id, device_id);
        let consumer = device_id.to_string();

        let autoclaim: redis::streams::StreamAutoClaimReply = conn
            .xautoclaim_options(
                &stream_key,
                CONSUMER_GROUP,
                &consumer,
                AUTOCLAIM_MIN_IDLE_MS,
                "0-0",
                StreamAutoClaimOptions::default().count(READ_COUNT),
            )
            .await?;

        if !autoclaim.claimed.is_empty() {
            return autoclaim
                .claimed
                .into_iter()
                .map(envelope_from_stream_id)
                .collect();
        }

        let read_opts = StreamReadOptions::default()
            .group(CONSUMER_GROUP, &consumer)
            .block(READ_BLOCK_MS)
            .count(READ_COUNT);

        let reply: Option<StreamReadReply> = conn
            .xread_options(&[stream_key.as_str()], &[">"], &read_opts)
            .await?;

        let Some(reply) = reply else {
            return Ok(Vec::new());
        };

        reply
            .keys
            .into_iter()
            .flat_map(|key| key.ids)
            .map(envelope_from_stream_id)
            .collect()
    }

    pub async fn ack(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        delivery_token: &str,
    ) -> Result<(), MailboxError> {
        self.store
            .run(|mut conn| async move {
                conn.xack::<_, _, _, usize>(
                    mailbox_key(user_id, device_id),
                    CONSUMER_GROUP,
                    &[delivery_token],
                )
                .await
            })
            .await?;

        tracing::trace!(%user_id, %device_id, %delivery_token, "envelope acked in redis");

        Ok(())
    }

    pub async fn register_push_token(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        token: &str,
    ) -> Result<(), MailboxError> {
        self.store
            .run(|mut conn| async move {
                conn.set::<_, _, ()>(push_token_key(user_id, device_id), token)
                    .await
            })
            .await?;

        tracing::debug!(%user_id, %device_id, "push token registered");

        Ok(())
    }

    pub async fn get_push_token(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<Option<String>, MailboxError> {
        self.store
            .run(|mut conn| async move { conn.get(push_token_key(user_id, device_id)).await })
            .await
    }

    pub async fn delete_push_token(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<(), MailboxError> {
        self.store
            .run(|mut conn| async move {
                conn.del::<_, usize>(push_token_key(user_id, device_id))
                    .await
            })
            .await?;

        tracing::debug!(%user_id, %device_id, "push token deleted");

        Ok(())
    }

    pub async fn heartbeat(&self, user_id: Uuid, device_id: Uuid) -> Result<(), MailboxError> {
        self.store
            .run(|mut conn| async move {
                conn.set_ex::<_, _, ()>(online_key(user_id, device_id), 1, ONLINE_TTL_SECONDS)
                    .await
            })
            .await
    }

    pub async fn is_online(&self, user_id: Uuid, device_id: Uuid) -> Result<bool, MailboxError> {
        self.store
            .run(|mut conn| async move { conn.exists(online_key(user_id, device_id)).await })
            .await
    }
}

fn devices_key(user_id: Uuid) -> String {
    format!("devices:{user_id}")
}

fn mailbox_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("mailbox:{user_id}:{device_id}")
}

fn push_token_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("push_token:{user_id}:{device_id}")
}

fn online_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("online:{user_id}:{device_id}")
}

fn envelope_items(envelope: &EncryptedEnvelope) -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("id", envelope.id.to_string().into_bytes()),
        ("chat_id", envelope.chat_id.to_string().into_bytes()),
        ("sender_id", envelope.sender_id.to_string().into_bytes()),
        (
            "sender_device_id",
            envelope.sender_device_id.to_string().into_bytes(),
        ),
        (
            "recipient_id",
            envelope.recipient_id.to_string().into_bytes(),
        ),
        (
            "recipient_device_id",
            envelope
                .recipient_device_id
                .map(|id| id.to_string())
                .unwrap_or_default()
                .into_bytes(),
        ),
        (
            "created_at",
            envelope
                .created_at
                .timestamp_millis()
                .to_string()
                .into_bytes(),
        ),
        (
            "event_type",
            (envelope.event_type.clone() as i32)
                .to_string()
                .into_bytes(),
        ),
        ("notify", envelope.notify.to_string().into_bytes()),
        ("ciphertext", envelope.ciphertext.clone()),
    ]
}

fn field_string(map: &HashMap<String, Value>, field: &str) -> Result<String, MailboxError> {
    field_bytes(map, field).and_then(|bytes| {
        String::from_utf8(bytes).map_err(|_| MailboxError::InvalidField {
            field: field.to_string(),
            reason: "not valid utf-8".to_string(),
        })
    })
}

fn field_bytes(map: &HashMap<String, Value>, field: &str) -> Result<Vec<u8>, MailboxError> {
    let value = map
        .get(field)
        .ok_or_else(|| MailboxError::MissingField(field.to_string()))?;

    redis::FromRedisValue::from_redis_value(value.clone()).map_err(|_| MailboxError::InvalidField {
        field: field.to_string(),
        reason: "not a bulk string".to_string(),
    })
}

fn parse_field<T: std::str::FromStr>(
    map: &HashMap<String, Value>,
    field: &str,
) -> Result<T, MailboxError> {
    field_string(map, field)?
        .parse::<T>()
        .map_err(|_| MailboxError::InvalidField {
            field: field.to_string(),
            reason: "could not be parsed".to_string(),
        })
}

fn envelope_from_stream_id(entry: StreamId) -> Result<(String, EncryptedEnvelope), MailboxError> {
    let map = &entry.map;

    let id: Uuid = parse_field(map, "id")?;
    let chat_id: Uuid = parse_field(map, "chat_id")?;
    let sender_id: Uuid = parse_field(map, "sender_id")?;
    let sender_device_id: Uuid = parse_field(map, "sender_device_id")?;
    let recipient_id: Uuid = parse_field(map, "recipient_id")?;

    let recipient_device_id_raw = field_string(map, "recipient_device_id")?;
    let recipient_device_id =
        if recipient_device_id_raw.is_empty() {
            None
        } else {
            Some(recipient_device_id_raw.parse::<Uuid>().map_err(|_| {
                MailboxError::InvalidField {
                    field: "recipient_device_id".to_string(),
                    reason: "could not be parsed".to_string(),
                }
            })?)
        };

    let notify: bool = parse_field(map, "notify")?;

    let created_at_ms: i64 = parse_field(map, "created_at")?;
    let created_at = DateTime::<Utc>::from_timestamp_millis(created_at_ms).ok_or_else(|| {
        MailboxError::InvalidField {
            field: "created_at".to_string(),
            reason: "out of range".to_string(),
        }
    })?;

    let event_type_raw: i32 = parse_field(map, "event_type")?;
    let event_type =
        EventType::try_from(event_type_raw).map_err(|e| MailboxError::InvalidField {
            field: "event_type".to_string(),
            reason: e.to_string(),
        })?;

    let ciphertext = field_bytes(map, "ciphertext")?;

    Ok((
        entry.id,
        EncryptedEnvelope {
            id,
            chat_id,
            sender_id,
            sender_device_id,
            recipient_id,
            recipient_device_id,
            created_at,
            event_type,
            notify,
            ciphertext,
        },
    ))
}
