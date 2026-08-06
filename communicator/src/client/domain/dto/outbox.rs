use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::outbox;
use crate::client::domain::dto::envelope::{EncryptedEnvelope, EventType};
use crate::client::domain::dto::errors::ValueError;

#[derive(Debug, Copy, Clone, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(i32)]
pub enum OutboxStatus {
    Pending = 0,
    Sent = 1,
}

impl TryFrom<i32> for OutboxStatus {
    type Error = ValueError;

    fn try_from(i: i32) -> Result<Self, ValueError> {
        match i {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Sent),
            n if n < 0 => Err(ValueError::LesserThanExpected),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

impl From<OutboxStatus> for i32 {
    fn from(status: OutboxStatus) -> Self {
        status as i32
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutboxEntry {
    pub id: Uuid,

    pub message_id: Uuid,
    pub chat_id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,

    pub event_type: EventType,
    pub ciphertext: Vec<u8>,
    pub status: OutboxStatus,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl OutboxEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        message_id: Uuid,
        chat_id: Uuid,
        sender_id: Uuid,
        recipient_id: Uuid,
        event_type: EventType,
        ciphertext: Vec<u8>,
        status: OutboxStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            message_id,
            chat_id,
            sender_id,
            recipient_id,
            event_type,
            ciphertext,
            status,
            created_at,
            updated_at,
        }
    }

    pub fn pending(message_id: Uuid, recipient_id: Uuid, envelope: &EncryptedEnvelope) -> Self {
        let now = Utc::now();

        Self {
            id: Uuid::new_v4(),
            message_id,
            chat_id: envelope.chat_id,
            sender_id: envelope.sender_id,
            recipient_id,
            event_type: envelope.event_type.clone(),
            ciphertext: envelope.ciphertext.clone(),
            status: OutboxStatus::Pending,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn from(model: outbox::Model) -> Result<Self, ValueError> {
        let event_type = EventType::try_from(model.event_type)?;
        let status = OutboxStatus::try_from(model.status)?;

        Ok(Self {
            id: model.id,
            message_id: model.message_id,
            chat_id: model.chat_id,
            sender_id: model.sender_id,
            recipient_id: model.recipient_id,
            event_type,
            ciphertext: model.ciphertext,
            status,
            created_at: model.created_at,
            updated_at: model.updated_at,
        })
    }

    pub fn into_model(&self) -> outbox::Model {
        outbox::Model {
            id: self.id,
            message_id: self.message_id,
            chat_id: self.chat_id,
            sender_id: self.sender_id,
            recipient_id: self.recipient_id,
            event_type: self.event_type.clone() as i32,
            ciphertext: self.ciphertext.clone(),
            status: self.status.into(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
