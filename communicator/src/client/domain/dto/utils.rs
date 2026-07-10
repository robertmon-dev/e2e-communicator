use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileMetadata {
    pub id: Uuid,

    pub file_name: String,
    pub file_size_bytes: i64,
    pub mime_type: Option<String>,

    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_secs: Option<i32>,

    pub thumbnail_link: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FileMetadata {
    pub fn new(
        id: Uuid,
        file_name: String,
        file_size_bytes: i64,
        mime_type: Option<String>,

        width: Option<i32>,
        height: Option<i32>,
        duration_secs: Option<i32>,

        thumbnail_link: Option<String>,

        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,

            file_name,
            file_size_bytes,
            mime_type,

            width,
            height,
            duration_secs,

            thumbnail_link,

            created_at,
            updated_at,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, ToSchema)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    In,
    NotIn,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    pub current_page: u64,
    pub per_page: u64,
    pub total_pages: u64,
    pub total_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeStep {
    Sec10,
    Sec20,
    Sec30,
    Sec45,
    Min1,
}

impl TimeStep {
    pub fn as_duration(&self) -> Duration {
        match self {
            Self::Sec10 => Duration::seconds(10),
            Self::Sec20 => Duration::seconds(20),
            Self::Sec30 => Duration::seconds(30),
            Self::Sec45 => Duration::seconds(45),
            Self::Min1 => Duration::seconds(60),
        }
    }

    pub fn as_secs(&self) -> u64 {
        match self {
            Self::Sec10 => 10,
            Self::Sec20 => 20,
            Self::Sec30 => 30,
            Self::Sec45 => 45,
            Self::Min1 => 60,
        }
    }
}
