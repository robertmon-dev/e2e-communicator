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
    pub message_id: Uuid,

    pub attachment_link: Option<String>,
    pub attachment_type: i32,

    pub file_name: String,
    pub file_size_bytes: i64,
    pub mime_type: Option<String>,

    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_secs: Option<i32>,

    pub thumbnail_link: Option<String>,

    pub created_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::message::Entity",
        from = "Column::MessageId",
        to = "super::message::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Message,
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
}

impl Related<super::message::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Message.def()
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

impl ActiveModelBehavior for ActiveModel {}
