use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "profile_photo_history")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    pub user_id: Uuid,

    pub created_at: DateTime<Utc>,

    pub attachment_link: Option<String>,

    pub profile_photo_type: i32,

    pub file_name: String,

    pub file_size_bytes: i64,
    pub mime_type: Option<String>,

    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_secs: Option<i32>,

    pub thumbnail_link: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
