use chrono::{DateTime, Utc};
use uuid::Uuid;

pub struct UpdateChatInput {
    pub id: Uuid,
    pub is_active: Option<bool>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub struct ChatRestrictions {
    pub restricted_chat_ids: Option<Vec<Uuid>>,
    pub restricted_user_ids: Option<Vec<Uuid>>,
    pub historical_memberships: Option<bool>,
    pub only_active: Option<bool>,
}
