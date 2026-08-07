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
    pub profile_photo_type: u8,

    pub file_metadata_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    User,

    #[sea_orm(
        belongs_to = "super::file_metadata::Entity",
        from = "Column::FileMetadataId",
        to = "super::file_metadata::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    FileMetadata,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::file_metadata::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FileMetadata.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
