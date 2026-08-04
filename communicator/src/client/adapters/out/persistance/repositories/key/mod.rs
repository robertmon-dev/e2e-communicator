pub mod create;
pub mod remove;
pub mod update;
pub mod utils;

use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use std::sync::Arc;
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::key;
use crate::client::domain::dto::key::Key;

pub struct KeyRepository {
    db: Arc<DatabaseConnection>,
}

impl KeyRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn get(&self, id: Uuid) -> Result<Key, DbErr> {
        let persisted_key = key::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Key with such ID has not been found".to_string())
            })?;

        Ok(Key::from(persisted_key))
    }

    pub async fn get_by_chat_id(&self, chat_id: Uuid) -> Result<Key, DbErr> {
        let persisted_key = key::Entity::find()
            .filter(key::Column::ChatId.eq(chat_id))
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Key for such chat has not been found".to_string())
            })?;

        Ok(Key::from(persisted_key))
    }

    pub async fn list(&self, restricted_chat_ids: Option<Vec<Uuid>>) -> Result<Vec<Key>, DbErr> {
        let mut query = key::Entity::find();

        if let Some(chat_ids) = restricted_chat_ids {
            query = query.filter(key::Column::ChatId.is_in(chat_ids));
        }

        let keys = query.all(self.db.as_ref()).await?;

        Ok(keys.into_iter().map(Key::from).collect())
    }
}
