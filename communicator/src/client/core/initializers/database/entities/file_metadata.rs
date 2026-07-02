use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_metadata")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    pub attachment_id: Option<Uuid>,
    pub profile_photo_id: Option<Uuid>,

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

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::attachment::Entity",
        from = "Column::AttachmentId",
        to = "super::attachment::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Attachment,

    #[sea_orm(
        belongs_to = "super::profile_photo_history::Entity",
        from = "Column::ProfilePhotoId",
        to = "super::profile_photo_history::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    ProfilePhotoHistory,
}

impl Related<super::attachment::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Attachment.def()
    }
}

impl Related<super::profile_photo_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProfilePhotoHistory.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
