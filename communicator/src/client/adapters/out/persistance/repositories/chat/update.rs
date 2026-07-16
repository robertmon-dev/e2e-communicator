use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use sea_orm::{DbErr, Set};
use std::collections::HashMap;
use uuid::Uuid;

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
}

impl Update for ChatRepository {
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
