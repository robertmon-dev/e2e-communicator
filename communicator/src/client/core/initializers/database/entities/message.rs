use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "messages")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub author_id: Uuid,
    pub chat_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub available_to: Option<DateTime<Utc>>,
    pub hidden: bool,
    pub is_reply: bool,
    pub reply_to_id: Option<Uuid>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::message_history_record::Entity")]
    MessageHistoryRecord,
}

impl Related<super::message_history_record::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::MessageHistoryRecord.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
