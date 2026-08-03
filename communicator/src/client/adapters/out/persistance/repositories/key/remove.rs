use sea_orm::{ColumnTrait, DbErr, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::key;
use crate::client::adapters::out::persistance::repositories::key::KeyRepository;
use crate::client::domain::dto::key::Key;

pub trait Remove {
    async fn remove_key(&self, id: Uuid) -> Result<Key, DbErr>;
    async fn remove_by_chat_id(&self, chat_id: Uuid) -> Result<Key, DbErr>;
}

impl Remove for KeyRepository {
    async fn remove_key(&self, id: Uuid) -> Result<Key, DbErr> {
        let key = self.get(id).await?;

        let delete_result = key::Entity::delete_by_id(id).exec(self.db.as_ref()).await?;

        if delete_result.rows_affected == 0 {
            return Err(DbErr::RecordNotUpdated);
        }

        Ok(key)
    }

    async fn remove_by_chat_id(&self, chat_id: Uuid) -> Result<Key, DbErr> {
        let key = self.get_by_chat_id(chat_id).await?;

        let delete_result = key::Entity::delete_many()
            .filter(key::Column::ChatId.eq(chat_id))
            .exec(self.db.as_ref())
            .await?;

        if delete_result.rows_affected == 0 {
            return Err(DbErr::RecordNotUpdated);
        }

        Ok(key)
    }
}
