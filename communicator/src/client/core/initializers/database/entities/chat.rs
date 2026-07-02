use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "chat")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub chat_type: u8,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::chat_history::Entity")]
    ChatHistory,

    #[sea_orm(has_many = "super::chat_member::Entity")]
    ChatMember,
}

impl Related<super::chat_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChatHistory.def()
    }
}

impl Related<super::chat_member::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChatMember.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
