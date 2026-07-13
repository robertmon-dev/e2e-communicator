use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::{chat, chat_history};
use crate::client::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::client::domain::dto::chat::Chat;

use sea_orm::{ColumnTrait, DbErr, EntityTrait, QueryFilter};

pub trait Remove {
    async fn remove_chat_history_record(&self, chat_history_record_id: Uuid)
    -> Result<Chat, DbErr>;
    async fn remove_chat(&self, chat_history_id: Uuid) -> Result<Chat, DbErr>;
}

impl Remove for ChatRepository {
    async fn remove_chat_history_record(
        &self,
        chat_history_record_id: Uuid,
    ) -> Result<Chat, DbErr> {
        let persisted_chat_record = chat_history::Entity::find_by_id(chat_history_record_id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("No chat history record with such ID".to_string())
            })?;

        let updated_record = chat_history::Entity::delete_by_id(chat_history_record_id)
            .exec(self.db.as_ref())
            .await?;

        if updated_record.rows_affected < 1 {
            return Err(DbErr::RecordNotUpdated);
        }

        self.get(persisted_chat_record.chat_id).await
    }

    async fn remove_chat(&self, chat_id: Uuid) -> Result<Chat, DbErr> {
        let persisted_chat = self.get(chat_id).await?;
        let updated_record = chat::Entity::delete_by_id(chat_id)
            .exec(self.db.as_ref())
            .await?;

        if updated_record.rows_affected < 1 {
            return Err(DbErr::RecordNotUpdated);
        }

        Ok(persisted_chat)
    }
}
