use chrono::{DateTime, Utc};
use uuid::Uuid;

pub struct UpdateChatInput {
    pub id: Uuid,
    pub is_active: Option<bool>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}
