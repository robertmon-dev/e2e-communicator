use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait,
    IntoActiveModel, LoaderTrait, Order, PaginatorTrait, QueryFilter, QueryOrder, Select,
    TransactionTrait,
};

use crate::client::adapters::out::persistance::entities::{chat_history, chat_member};
use crate::client::adapters::out::persistance::repositories::utils::safe_bulk_insert;
use crate::client::{
    adapters::out::persistance::repositories::chat::ChatRepository,
    domain::dto::chat::{Chat, UnstructurizedChat},
};

pub trait Create {
    async fn create(&self, chat: Chat) -> Result<(), DbErr>;
}

impl Create for ChatRepository {
    async fn create(&self, chat: Chat) -> Result<(), DbErr> {
        let (chat_model, chat_history) = UnstructurizedChat::from(chat);

        let txn = self.db.begin().await?;

        let chat_model_active = chat_model.into_active_model();
        chat_model_active.insert(&txn).await?;

        let mut chat_members_active = Vec::new();
        let mut chat_history_active = Vec::new();

        for (chat_history_record, chat_members) in chat_history.into_iter() {
            chat_history_active.push(chat_history_record.into_active_model());

            chat_members
                .into_iter()
                .for_each(|model| chat_members_active.push(model.into_active_model()));
        }

        safe_bulk_insert::<chat_history::Entity, _, _>(&txn, chat_history_active).await?;
        safe_bulk_insert::<chat_member::Entity, _, _>(&txn, chat_members_active).await?;

        txn.commit().await?;

        Ok(())
    }
}
