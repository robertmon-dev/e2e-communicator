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

pub struct Mailbox {
    store: Arc<MailboxStore>,
}

impl Mailbox {
    pub fn new(store: Arc<MailboxStore>) -> Self {
        Self { store }
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

        if let Err(err) = create_result {
            if err.code() != Some("BUSYGROUP") {
                return Err(err.into());
            }
        }

        Ok(())
    }

    pub async fn fan_out(&self, envelope: &EncryptedEnvelope) -> Result<(), MailboxError> {
        let mut conn = self.store.connection().await?;

        let device_ids: Vec<String> = conn.smembers(devices_key(envelope.recipient_id)).await?;
        let items = envelope_items(envelope);

        for device_id in device_ids {
            let stream_key = format!("mailbox:{}:{}", envelope.recipient_id, device_id);
            let _: Option<String> = conn.xadd(&stream_key, "*", &items).await?;
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

        Ok(())
    }
}

fn devices_key(user_id: Uuid) -> String {
    format!("devices:{user_id}")
}

fn mailbox_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("mailbox:{user_id}:{device_id}")
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

    redis::FromRedisValue::from_redis_value(value).map_err(|_| MailboxError::InvalidField {
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
