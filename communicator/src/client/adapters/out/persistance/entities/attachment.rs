use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "attachment")]
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
    pub updated_at: DateTime<Utc>,
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
        belongs_to = "super::chat::Entity",
        from = "Column::ChatId",
        to = "super::chat::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Chat,

    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::AuthorId",
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

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert {
            self.updated_at = sea_orm::Set(chrono::Utc::now());
        }

        Ok(self)
    }
}
