use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::Value;
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

#[derive(Debug, Clone, PartialEq)]
pub struct PendingNotification {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub chat_id: Uuid,
    pub envelope_id: Uuid,
    pub event_type: EventType,
}

impl PendingNotification {
    pub fn encode(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.user_id,
            self.device_id,
            self.chat_id,
            self.envelope_id,
            self.event_type.clone() as i32
        )
    }

    pub fn decode(raw: &str) -> Result<Self, MailboxError> {
        let parts: Vec<&str> = raw.split(':').collect();
        let [user_id, device_id, chat_id, envelope_id, event_type] = parts[..] else {
            return Err(MailboxError::InvalidField {
                field: "mailbox:pending payload".to_string(),
                reason: format!("expected 5 colon-separated fields, got {}", parts.len()),
            });
        };

        let parse_field_uuid = |value: &str, field: &str| {
            Uuid::parse_str(value).map_err(|_| MailboxError::InvalidField {
                field: field.to_string(),
                reason: "not a valid uuid".to_string(),
            })
        };

        let event_type_raw: i32 = event_type.parse().map_err(|_| MailboxError::InvalidField {
            field: "event_type".to_string(),
            reason: "not a valid integer".to_string(),
        })?;

        Ok(Self {
            user_id: parse_field_uuid(user_id, "user_id")?,
            device_id: parse_field_uuid(device_id, "device_id")?,
            chat_id: parse_field_uuid(chat_id, "chat_id")?,
            envelope_id: parse_field_uuid(envelope_id, "envelope_id")?,
            event_type: EventType::try_from(event_type_raw).map_err(|e| {
                MailboxError::InvalidField {
                    field: "event_type".to_string(),
                    reason: e.to_string(),
                }
            })?,
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
        let mut conn = self.store.connection().await?;

        let _: usize = conn
            .sadd(devices_key(user_id), device_id.to_string())
            .await?;

        let stream_key = mailbox_key(user_id, device_id);
        let create_result: redis::RedisResult<()> = conn
            .xgroup_create_mkstream(&stream_key, CONSUMER_GROUP, "0")
            .await;

        if let Err(err) = create_result
            && err.code() != Some("BUSYGROUP")
        {
            return Err(err.into());
        }

        tracing::debug!(%user_id, %device_id, "device registered");

        Ok(())
    }

    pub async fn fan_out(&self, envelope: &EncryptedEnvelope) -> Result<(), MailboxError> {
        let mut conn = self.store.connection().await?;

        let device_ids: Vec<String> = conn.smembers(devices_key(envelope.recipient_id)).await?;

        tracing::debug!(
            envelope_id = %envelope.id,
            recipient_id = %envelope.recipient_id,
            device_count = device_ids.len(),
            "fanning out envelope"
        );

        let items = envelope_items(envelope);
        let notify = matches!(
            envelope.event_type,
            EventType::Message | EventType::Reaction
        );

        for device_id in device_ids {
            let stream_key = format!("mailbox:{}:{}", envelope.recipient_id, device_id);
            let _: Option<String> = conn.xadd(&stream_key, "*", &items).await?;

            if !notify {
                continue;
            }

            let Ok(device_uuid) = Uuid::parse_str(&device_id) else {
                tracing::warn!(%device_id, "skipping pending notification for malformed device id");
                continue;
            };

            let notification = PendingNotification {
                user_id: envelope.recipient_id,
                device_id: device_uuid,
                chat_id: envelope.chat_id,
                envelope_id: envelope.id,
                event_type: envelope.event_type.clone(),
            };

            if let Err(err) = conn
                .publish::<_, _, usize>(PENDING_CHANNEL, notification.encode())
                .await
            {
                tracing::warn!(error = %err, %device_id, "failed to publish mailbox:pending event");
            }
        }

        Ok(())
    }

    pub async fn read_next(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<Vec<(String, EncryptedEnvelope)>, MailboxError> {
        let mut conn = self.store.connection().await?;
        let stream_key = mailbox_key(user_id, device_id);
        let consumer = device_id.to_string();

        let autoclaim_opts = StreamAutoClaimOptions::default().count(READ_COUNT);
        let autoclaim: redis::streams::StreamAutoClaimReply = conn
            .xautoclaim_options(
                &stream_key,
                CONSUMER_GROUP,
                &consumer,
                AUTOCLAIM_MIN_IDLE_MS,
                "0-0",
                autoclaim_opts,
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
        let mut conn = self.store.connection().await?;
        let stream_key = mailbox_key(user_id, device_id);

        let _: usize = conn
            .xack(&stream_key, CONSUMER_GROUP, &[delivery_token])
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
        let mut conn = self.store.connection().await?;
        let _: () = conn.set(push_token_key(user_id, device_id), token).await?;

        tracing::debug!(%user_id, %device_id, "push token registered");

        Ok(())
    }

    pub async fn get_push_token(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<Option<String>, MailboxError> {
        let mut conn = self.store.connection().await?;
        let token: Option<String> = conn.get(push_token_key(user_id, device_id)).await?;
        Ok(token)
    }

    pub async fn delete_push_token(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<(), MailboxError> {
        let mut conn = self.store.connection().await?;
        let _: usize = conn.del(push_token_key(user_id, device_id)).await?;

        tracing::debug!(%user_id, %device_id, "push token deleted");

        Ok(())
    }

    pub async fn heartbeat(&self, user_id: Uuid, device_id: Uuid) -> Result<(), MailboxError> {
        let mut conn = self.store.connection().await?;
        let _: () = conn
            .set_ex(online_key(user_id, device_id), 1, ONLINE_TTL_SECONDS)
            .await?;
        Ok(())
    }

    pub async fn is_online(&self, user_id: Uuid, device_id: Uuid) -> Result<bool, MailboxError> {
        let mut conn = self.store.connection().await?;
        let exists: bool = conn.exists(online_key(user_id, device_id)).await?;
        Ok(exists)
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
            "recipient_id",
            envelope.recipient_id.to_string().into_bytes(),
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
    let recipient_id: Uuid = parse_field(map, "recipient_id")?;

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
            recipient_id,
            created_at,
            event_type,
            ciphertext,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::initializers::settings::RedisSettings;

    async fn test_mailbox() -> Mailbox {
        let settings = RedisSettings {
            sentinel_nodes: vec![
                "localhost:26379".to_string(),
                "localhost:26380".to_string(),
                "localhost:26381".to_string(),
            ],
            service_name: "mymaster".to_string(),
            password: None,
        };
        let store = MailboxStore::new(&settings)
            .expect("failed to build MailboxStore - is `make redis-up` running?");
        Mailbox::new(store)
    }

    #[tokio::test]
    async fn register_and_get_push_token_round_trips() {
        let mailbox = test_mailbox().await;
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        mailbox
            .register_push_token(user_id, device_id, "test-token-123")
            .await
            .unwrap();

        let token = mailbox.get_push_token(user_id, device_id).await.unwrap();
        assert_eq!(token, Some("test-token-123".to_string()));
    }

    #[tokio::test]
    async fn get_push_token_returns_none_when_unregistered() {
        let mailbox = test_mailbox().await;
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        let token = mailbox.get_push_token(user_id, device_id).await.unwrap();
        assert_eq!(token, None);
    }

    #[tokio::test]
    async fn delete_push_token_removes_it() {
        let mailbox = test_mailbox().await;
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        mailbox
            .register_push_token(user_id, device_id, "test-token-123")
            .await
            .unwrap();
        mailbox.delete_push_token(user_id, device_id).await.unwrap();

        let token = mailbox.get_push_token(user_id, device_id).await.unwrap();
        assert_eq!(token, None);
    }

    #[tokio::test]
    async fn is_online_false_before_heartbeat() {
        let mailbox = test_mailbox().await;
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        let online = mailbox.is_online(user_id, device_id).await.unwrap();
        assert!(!online);
    }

    #[tokio::test]
    async fn is_online_true_after_heartbeat() {
        let mailbox = test_mailbox().await;
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        mailbox.heartbeat(user_id, device_id).await.unwrap();

        let online = mailbox.is_online(user_id, device_id).await.unwrap();
        assert!(online);
    }

    #[test]
    fn pending_notification_round_trips_through_encode_decode() {
        let notification = PendingNotification {
            user_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            chat_id: Uuid::new_v4(),
            envelope_id: Uuid::new_v4(),
            event_type: EventType::Message,
        };

        let encoded = notification.encode();
        let decoded = PendingNotification::decode(&encoded).unwrap();

        assert_eq!(decoded, notification);
    }

    #[test]
    fn pending_notification_decode_rejects_malformed_payload() {
        let result = PendingNotification::decode("not-enough-fields");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fan_out_publishes_pending_notification_for_message_events() {
        use tokio_stream::StreamExt;

        let mailbox = test_mailbox().await;
        let recipient_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        mailbox
            .register_device(recipient_id, device_id)
            .await
            .unwrap();

        let mut pubsub = mailbox.pubsub_connection().await.unwrap();
        pubsub.subscribe(PENDING_CHANNEL).await.unwrap();
        let mut stream = pubsub.on_message();

        let envelope = EncryptedEnvelope {
            id: Uuid::new_v4(),
            chat_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            recipient_id,
            created_at: chrono::Utc::now(),
            event_type: EventType::Message,
            ciphertext: vec![1, 2, 3],
        };

        mailbox.fan_out(&envelope).await.unwrap();

        // The pending channel is global, so a concurrently-running test's own
        // publish can arrive first — loop until we see the notification for
        // *this* test's device, rather than assuming the first message is ours.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        let notification = loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for pending notification"
            );

            let msg = tokio::time::timeout(remaining, stream.next())
                .await
                .expect("timed out waiting for pending notification")
                .expect("pubsub stream ended unexpectedly");

            let payload: String = msg.get_payload().unwrap();
            let notification = PendingNotification::decode(&payload).unwrap();

            if notification.user_id == recipient_id && notification.device_id == device_id {
                break notification;
            }
        };

        assert_eq!(notification.chat_id, envelope.chat_id);
        assert_eq!(notification.envelope_id, envelope.id);
    }

    #[tokio::test]
    async fn fan_out_does_not_publish_for_receipt_events() {
        use tokio_stream::StreamExt;

        let mailbox = test_mailbox().await;
        let recipient_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        mailbox
            .register_device(recipient_id, device_id)
            .await
            .unwrap();

        let mut pubsub = mailbox.pubsub_connection().await.unwrap();
        pubsub.subscribe(PENDING_CHANNEL).await.unwrap();
        let mut stream = pubsub.on_message();

        let envelope = EncryptedEnvelope {
            id: Uuid::new_v4(),
            chat_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            recipient_id,
            created_at: chrono::Utc::now(),
            event_type: EventType::DeliveryReceipt,
            ciphertext: vec![1, 2, 3],
        };

        mailbox.fan_out(&envelope).await.unwrap();

        // Global channel, so other concurrently-running tests legitimately
        // publish on it too — only fail if a notification for *this* test's
        // own device shows up.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            let Ok(Some(msg)) = tokio::time::timeout(remaining, stream.next()).await else {
                break;
            };

            let payload: String = msg.get_payload().unwrap();
            if let Ok(notification) = PendingNotification::decode(&payload)
                && notification.user_id == recipient_id
                && notification.device_id == device_id
            {
                panic!(
                    "expected no pending notification for a receipt event, got one for this device"
                );
            }
        }
    }
}
