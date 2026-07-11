use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use sea_orm::{DbErr, Set};
use std::collections::HashMap;
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::{message, message_history};
use crate::client::adapters::out::persistance::repositories::message::MessageRepository;
use crate::client::adapters::out::persistance::repositories::message::utils::UpdateMessage;
use crate::client::domain::dto::filter_messages::ListMessagesFilter;
use crate::client::domain::dto::message::{Message, MessageContent};
use crate::client::domain::dto::utils::PaginationMeta;

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
    async fn update_message(&self, update_input: UpdateMessage) -> Result<Message, DbErr>;
    async fn update_messages(
        &self,
        update_input: Vec<UpdateMessage>,
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

    async fn update_message(&self, update_input: UpdateMessage) -> Result<Message, DbErr> {
        let message = message::Entity::find_by_id(update_input.id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound(format!("Message with ID {} not found", update_input.id))
            })?;

        let mut active_message: message::ActiveModel = message.into();

        if let Some(available) = update_input.available_to {
            active_message.available_to = sea_orm::Set(Some(available));
        }

        if let Some(hidden_switch) = update_input.hidden {
            active_message.hidden = sea_orm::Set(hidden_switch);
        }

        active_message.update(self.db.as_ref()).await?;

        self.get(update_input.id).await
    }

    async fn update_messages(
        &self,
        update_inputs: Vec<UpdateMessage>,
    ) -> Result<Vec<Message>, DbErr> {
        if update_inputs.is_empty() {
            return Ok(Vec::new());
        }

        let message_ids: Vec<Uuid> = update_inputs.iter().map(|u| u.id).collect();
        let messages_length = message_ids.len() as u64;

        let mut updates_by_id: HashMap<Uuid, UpdateMessage> =
            update_inputs.into_iter().map(|u| (u.id, u)).collect();

        let messages = message::Entity::find()
            .filter(message::Column::Id.is_in(message_ids.clone()))
            .all(self.db.as_ref())
            .await?;

        let txn = self.db.begin().await?;

        for message in messages {
            if let Some(update_data) = updates_by_id.remove(&message.id) {
                let mut active_message: message::ActiveModel = message.into();

                if let Some(available) = update_data.available_to {
                    active_message.available_to = sea_orm::Set(Some(available));
                }

                if let Some(hidden_switch) = update_data.hidden {
                    active_message.hidden = sea_orm::Set(hidden_switch);
                }

                active_message.update(&txn).await?;
            }
        }
        txn.commit().await?;

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

        let messages = self
            .list(filter, pagination, None, None, None, false)
            .await?;

        Ok(messages)
    }
}
