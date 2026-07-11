use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::{message, message_history};
use crate::client::adapters::out::persistance::repositories::message::MessageRepository;
use crate::client::domain::dto::filter_messages::ListMessagesFilter;
use crate::client::domain::dto::message::Message;
use crate::client::domain::dto::utils::PaginationMeta;

use sea_orm::{ColumnTrait, DbErr, EntityTrait, QueryFilter};

pub trait Remove {
    async fn remove_history_record(
        &self,
        message_history_record_id: Uuid,
    ) -> Result<Message, DbErr>;
    async fn remove_messages(&self, message_ids: Vec<Uuid>) -> Result<Vec<Message>, DbErr>;
    async fn remove_message(&self, message_id: Uuid) -> Result<Message, DbErr>;
}

impl Remove for MessageRepository {
    async fn remove_history_record(
        &self,
        message_history_record_id: Uuid,
    ) -> Result<Message, DbErr> {
        let history_record = message_history::Entity::find_by_id(message_history_record_id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Not found message history record with such ID".to_string())
            })?;

        let delete_result = message_history::Entity::delete_by_id(message_history_record_id)
            .exec(self.db.as_ref())
            .await?;

        if delete_result.rows_affected == 0 {
            return Err(DbErr::RecordNotUpdated);
        }

        self.get(history_record.message_id).await
    }

    async fn remove_message(&self, message_id: Uuid) -> Result<Message, DbErr> {
        let message = self.get(message_id).await?;
        let delete_result = message::Entity::delete_by_id(message_id)
            .exec(self.db.as_ref())
            .await?;

        if delete_result.rows_affected == 0 {
            return Err(DbErr::RecordNotUpdated);
        }

        Ok(message)
    }

    async fn remove_messages(&self, message_ids: Vec<Uuid>) -> Result<Vec<Message>, DbErr> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }

        let messages_length = message_ids.len() as u64;
        let filter = ListMessagesFilter {
            page: Some(0),
            per_page: Some(messages_length),
            ..Default::default()
        };

        let pagination = PaginationMeta {
            current_page: 0,
            per_page: messages_length,
            total_pages: 1,
            total_count: messages_length,
        };

        let messages_to_remove = self
            .list(filter, pagination, None, None, None, false)
            .await?;

        message::Entity::delete_many()
            .filter(message::Column::Id.is_in(message_ids))
            .exec(self.db.as_ref())
            .await?;

        Ok(messages_to_remove)
    }
}
