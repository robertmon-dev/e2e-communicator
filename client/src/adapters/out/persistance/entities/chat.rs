use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "chat")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    pub is_active: bool,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::chat_history::Entity")]
    ChatHistory,

    #[sea_orm(has_many = "super::attachment::Entity")]
    Attachment,

    #[sea_orm(has_many = "super::reaction::Entity")]
    Reaction,

    #[sea_orm(has_many = "super::message::Entity")]
    Message,
}

impl Related<super::attachment::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Attachment.def()
    }
}

impl Related<super::chat_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChatHistory.def()
    }
}

impl Related<super::reaction::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Reaction.def()
    }
}

impl Related<super::message::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Message.def()
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
