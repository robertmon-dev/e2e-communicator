use chrono::{DateTime, Utc};
use uuid::Uuid;

pub struct UpdateMessage {
    pub id: Uuid,
    pub hidden: Option<bool>,
    pub available_to: Option<DateTime<Utc>>,
}
