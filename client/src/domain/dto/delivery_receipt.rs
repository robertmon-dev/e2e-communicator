use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
pub struct DeliveryReceipt {
    pub id: Uuid,
    pub message_id: Uuid,
    pub chat_id: Uuid,
    pub user_id: Uuid,
}
