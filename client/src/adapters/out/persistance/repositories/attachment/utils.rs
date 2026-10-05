use crate::domain::dto::message::AttachmentType;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct AttachmentRestrictions {
    pub restricted_chat_ids: Option<Vec<Uuid>>,
    pub restricted_user_ids: Option<Vec<Uuid>>,
    pub restricted_message_ids: Option<Vec<Uuid>>,
    pub restricted_message_history_ids: Option<Vec<Uuid>>,
    pub restricted_metadata_ids: Option<Vec<Uuid>>,
}

pub struct AttachmentUpdateInput {
    pub id: Uuid,
    pub attachment_type: Option<AttachmentType>,

    pub available_to: Option<DateTime<Utc>>,

    pub views: Option<i32>,
}

pub struct FileMetadataUpdateInput {
    pub id: Uuid,

    pub file_name: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub mime_type: Option<String>,

    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_secs: Option<i32>,

    pub thumbnail_link: Option<String>,
}
