use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: Option<String>,
    pub surname: Option<String>,
    pub nickname: String,
    pub phone: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::profile_photo_history::Entity")]
    ProfilePhotoHistory,
    #[sea_orm(has_many = "super::user_overlay_history::Entity")]
    UserOverlayHistory,
    #[sea_orm(has_many = "super::message::Entity")]
    Message,
    #[sea_orm(has_many = "super::reaction::Entity")]
    Reaction,
    #[sea_orm(has_many = "super::chat_member::Entity")]
    ChatMember,
}

impl Related<super::profile_photo_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProfilePhotoHistory.def()
    }
}

impl Related<super::user_overlay_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserOverlayHistory.def()
    }
}

impl Related<super::message::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Message.def()
    }
}

impl Related<super::reaction::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Reaction.def()
    }
}

impl Related<super::chat_member::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChatMember.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
