use chrono::{DateTime, Utc};
use sea_orm::EntityTrait;
use sea_orm::{DbErr, Set};
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::message;
use crate::client::adapters::out::persistance::entities::message_history;
use crate::client::adapters::out::persistance::repositories::message::MessageRepository;
use crate::client::domain::dto::errors::ValueError;
use crate::client::domain::dto::message::{Message, MessageContent};

pub trait Update {
    async fn add_history_record(
        &self,
        message_id: Uuid,
        message_history_record: MessageContent,
    ) -> Result<Message, DbErr>;
    async fn remove_history_record(
        &self,
        message_history_record_id: Uuid,
    ) -> Result<Message, DbErr>;
    fn update_message(message_id: Uuid, message: Message) -> Result<Message, DbErr>;
    fn update_messages(
        message_ids: Vec<Uuid>,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>, DbErr>;
}

impl Update for MessageRepository {
    async fn add_history_record(
        &self,
        message_id: Uuid,
        message_history_record: MessageContent,
    ) -> Result<Message, DbErr> {
        let new_history = message_history::ActiveModel {
            id: Set(Uuid::new_v4()),
            message_id: Set(message_id),
            text: Set(message_history_record.text),
            created_at: Set(Utc::now()),
        };

        message_history::Entity::insert(new_history)
            .exec(self.db.as_ref())
            .await?;

        self.get(message_id).await
    }

    async fn remove_history_record(
        &self,
        message_history_record_id: Uuid,
    ) -> Result<Message, DbErr> {
        let persisted_message = message_history::Entity::find_by_id(message_history_record_id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Not found message history record with such ID".to_string())
            })?;

        let removed_messages = message_history::Entity::delete_by_id(message_history_record_id)
            .exec(self.db.as_ref())
            .await?;

        if removed_messages.rows_affected < 1 {
            return Err(DbErr::RecordNotUpdated);
        }

        self.get(persisted_message.message_id).await
    }

    fn update_message(message_id: Uuid, message: Message) -> Result<Message, DbErr> {
        Err(DbErr::RecordNotUpdated)
    }

    fn update_messages(
        message_ids: Vec<Uuid>,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>, DbErr> {
        Err(DbErr::RecordNotUpdated)
    }
}
