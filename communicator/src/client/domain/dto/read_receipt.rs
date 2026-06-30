use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
pub struct ReadReceipt {
    pub id: Uuid,
    pub message_id: Uuid,
    pub chat_id: Uuid,
}
