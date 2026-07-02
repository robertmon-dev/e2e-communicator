use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "attachments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    pub chat_id: Uuid,

    pub author_id: Uuid,
    pub message_history_id: Uuid,

    pub attachment_link: Option<String>,
    pub attachment_type: i32,

    pub file_metadata_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::message_history::Entity",
        from = "Column::MessageHistoryId",
        to = "super::message_history::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    MessageHistoryRecord,

    #[sea_orm(
        belongs_to = "super::chat::entity",
        from = "Column::ChatId",
        to = "super::chat::Column::id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Chat,

    #[sea_orm(
        belongs_to = "super::user::entity",
        from = "Column::AuthorId",
        to = "super::user::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    User,

    #[sea_orm(has_one = "super::file_metadata::Entity")]
    FileMetadata,
}

impl Related<super::message_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::MessageHistoryRecord.def()
    }
}

impl Related<super::chat::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Chat.def()
    }
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
