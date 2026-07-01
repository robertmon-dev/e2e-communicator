use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum EventType {
    Message = 0,
    Reaction = 1,
    ReadReceipt = 2,
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
