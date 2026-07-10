use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::{message, message_history};
use crate::client::adapters::out::persistance::repositories::message::MessageRepository;
use crate::client::domain::dto::errors::ValueError;
use crate::client::domain::dto::message::{Message, MessageContent};
use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, LoaderTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select,
};

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
        Err(DbErr::RecordNotUpdated)
    }
}
