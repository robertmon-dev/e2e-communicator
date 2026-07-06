use sea_orm::{DatabaseConnection, DbErr, EntityTrait, Select};
use uuid::Uuid;

use std::sync::Arc;

use crate::client::adapters::out::persistance::entities::message;

pub struct MessageRepository {
    db: Arc<DatabaseConnection>,
}

impl MessageRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<message::Model>, DbErr> {
        message::Entity::find_by_id(id).one(self.db.as_ref()).await
    }

    pub fn build_base_query() -> Select<message::Entity> {}
}
