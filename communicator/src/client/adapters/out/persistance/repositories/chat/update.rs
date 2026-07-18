use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, TransactionTrait,
};
use sea_orm::{DbErr, Set};
use std::collections::HashMap;
use uuid::Uuid;

use super::utils::UpdateChatInput;
use crate::client::adapters::out::persistance::entities::chat;
use crate::client::adapters::out::persistance::repositories::chat::utils::ChatRestrictions;
use crate::client::domain::dto::filter_chat::ListChatsFilter;
use crate::client::domain::dto::utils::PaginationMeta;
use crate::client::{
    adapters::out::persistance::entities::{chat_history, chat_member},
    domain::dto::chat::{Chat, ChatState},
};

use super::ChatRepository;

pub trait Update {
    async fn add_history_record(
        &self,
        chat_id: Uuid,
        chat_history_record: ChatState,
    ) -> Result<Chat, DbErr>;

    async fn remove_chat_history_record(&self, chat_history_record_id: Uuid)
    -> Result<Chat, DbErr>;

    async fn update_chat(&self, update_input: UpdateChatInput) -> Result<Chat, DbErr>;
    async fn update_chats(&self, update_input: Vec<UpdateChatInput>) -> Result<Vec<Chat>, DbErr>;
}

impl Update for ChatRepository {
    async fn update_chat(&self, update_input: UpdateChatInput) -> Result<Chat, DbErr> {
        let persited_chat = chat::Entity::find_by_id(update_input.id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("No chat persited with such ID".to_string()))?;

        let mut active_chat = persited_chat.into_active_model();

        if let Some(activness) = update_input.is_active {
            active_chat.is_active = Set(activness);
        }

        if let Some(created_date) = update_input.created_at {
            active_chat.created_at = Set(created_date);
        }

        if let Some(updated_date) = update_input.updated_at {
            active_chat.updated_at = Set(updated_date);
        }

        active_chat.update(self.db.as_ref()).await?;

        self.get(update_input.id).await
    }

    async fn update_chats(&self, update_input: Vec<UpdateChatInput>) -> Result<Vec<Chat>, DbErr> {
        let chat_ids: Vec<Uuid> = update_input.iter().map(|input| input.id).collect();
        let chats_length = chat_ids.len() as u64;

        let persisted_chats = chat::Entity::find()
            .filter(chat::Column::Id.is_in(chat_ids.clone()))
            .all(self.db.as_ref())
            .await?;

        if persisted_chats.len() as u64 != chats_length {
            return Err(DbErr::RecordNotFound(format!(
                "Not found all of requested chats, missing {} IDs",
                persisted_chats.len().abs_diff(update_input.len())
            )));
        }

        let update_inputs_by_id: HashMap<Uuid, UpdateChatInput> =
            update_input.into_iter().map(|i| (i.id, i)).collect();

        let txn = self.db.begin().await?;

        for chat in persisted_chats.into_iter() {
            if let Some(update_input) = update_inputs_by_id.get(&chat.id) {
                let mut active_chat = chat.into_active_model();

                if let Some(activness) = update_input.is_active {
                    active_chat.is_active = Set(activness);
                }

                if let Some(created_date) = update_input.created_at {
                    active_chat.created_at = Set(created_date);
                }

                if let Some(update_date) = update_input.updated_at {
                    active_chat.updated_at = Set(update_date);
                }

                active_chat.update(&txn).await?;
            }
        }

        txn.commit().await?;

        let filter = ListChatsFilter {
            page: Some(0),
            per_page: Some(chats_length),
            ..Default::default()
        };

        let pagination = PaginationMeta {
            current_page: 0,
            per_page: chats_length,
            total_pages: 1,
            total_count: chats_length,
        };

        let restrictions = ChatRestrictions {
            restricted_chat_ids: Some(chat_ids),
            restricted_user_ids: None,
            historical_memberships: None,
            only_active: None,
        };

        self.list(filter, pagination, restrictions).await
    }

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

    async fn add_history_record(
        &self,
        chat_id: Uuid,
        chat_history_record: ChatState,
    ) -> Result<Chat, DbErr> {
        let name = chat_history_record
            .name
            .clone()
            .ok_or(DbErr::RecordNotInserted)?;

        let chat_type_u8 = chat_history_record.chat_type.into();
        let history_id = Uuid::new_v4();

        let history_record_active = chat_history::ActiveModel {
            id: Set(history_id),
            chat_id: Set(chat_id),
            created_at: Set(Utc::now()),
            chat_type: Set(chat_type_u8),
            name: Set(name),
            chat_theme: Set(chat_history_record.chat_theme.map(|url| url.to_string())),
        };

        let txn = self.db.begin().await?;

        chat_history::Entity::insert(history_record_active)
            .exec(&txn)
            .await?;

        let mut members_active = Vec::new();
        for (user_id, role) in chat_history_record.members.into_iter() {
            members_active.push(chat_member::ActiveModel {
                id: Set(Uuid::new_v4()),
                chat_history_id: Set(history_id),
                user_id: Set(user_id),
                role: Set(role.into()),
            });
        }

        if !members_active.is_empty() {
            chat_member::Entity::insert_many(members_active)
                .exec(&txn)
                .await?;
        }

        txn.commit().await?;

        let updated_chat = self.get(chat_id).await?;
        Ok(updated_chat)
    }
}
