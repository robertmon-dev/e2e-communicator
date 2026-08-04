use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::convert::TryFrom;
use uuid::Uuid;

use crate::client::domain::dto::errors::ValueError;

#[derive(Debug, Clone, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum EventType {
    Message = 0,
    Reaction = 1,
    ReadReceipt = 2,
    OverlayReceipt = 3,
    InvitationReceipt = 4,
}

impl TryFrom<i32> for EventType {
    type Error = ValueError;

    fn try_from(i: i32) -> Result<Self, ValueError> {
        match i {
            0 => Ok(EventType::Message),
            1 => Ok(EventType::Reaction),
            2 => Ok(EventType::ReadReceipt),
            3 => Ok(EventType::OverlayReceipt),
            4 => Ok(EventType::InvitationReceipt),
            i if i < 0 => Err(ValueError::LesserThanExpected),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncryptedEnvelope {
    pub id: Uuid,
    pub chat_id: Uuid,
    pub sender_id: Uuid,
    pub created_at: DateTime<Utc>,

    pub event_type: EventType,

    pub ciphertext: Vec<u8>,
}
