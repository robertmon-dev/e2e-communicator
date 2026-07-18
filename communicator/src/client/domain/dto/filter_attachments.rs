use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::client::domain::dto::message::AttachmentType;

use super::utils::{FilterOperator, SortDir};

#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub enum SortBy {
    CreatedAt,
    UpdatedAt,
    AvailableTo,
    AuthorId,
    ChatId,
    Id,
    AttachmentLink,
    AttachmentType,
    FileMetadataId,
    ViewsLeft,
}

pub struct ListAttachmentsFilter {
    pub page: Option<u64>,
    pub per_page: Option<u64>,

    pub attachment_type: Option<AttachmentType>,

    pub action_operator: Option<FilterOperator>,
    pub file_name: String,
    pub file_size_bytes: i64,
    pub mime_type: Option<String>,

    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_secs: Option<i32>,

    pub thumbnail_link: Option<String>,

    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,

    pub modified_from: Option<DateTime<Utc>>,
    pub modified_to: Option<DateTime<Utc>>,

    pub available_to: Option<DateTime<Utc>>,

    pub sort_by: Option<SortBy>,
    pub sort_dir: Option<SortDir>,

    pub search_operator: Option<FilterOperator>,

    pub left_views: Option<u64>,
    pub limited_view: Option<bool>,
    pub search_in_latest: bool,
}
