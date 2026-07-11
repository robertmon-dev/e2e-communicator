use chrono::{DateTime, Utc};
use uuid::Uuid;

pub struct UpdateMessage {
    pub id: Uuid,
    pub available_to: Option<DateTime<Utc>>,
    pub hidden: Option<bool>,
}
