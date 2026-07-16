pub mod create;
pub mod remove;
pub mod update;
pub mod utils;

use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, LoaderTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, Select,
};
use uuid::Uuid;

use std::collections::HashMap;
use std::sync::Arc;

use crate::client::adapters::out::persistance::entities::{chat, chat_history, chat_member};
use crate::client::domain::dto::chat::Chat;
use crate::client::domain::dto::filter_chat::{ListChatsFilter, SortBy};
use crate::client::domain::dto::utils::{PaginationMeta, SortDir};

use super::utils::{ConditionExt, apply_string_filter};

pub struct ChatRepository {
    db: Arc<DatabaseConnection>,
}

impl ChatRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    fn build_base_query(
        &self,
        filter: ListChatsFilter,
        restricted_chat_ids: Option<Vec<Uuid>>,
        restricted_user_ids: Option<Vec<Uuid>>,
        historical_memberships: Option<bool>,
        only_active: Option<bool>,
    ) -> Select<chat::Entity> {
        let is_historical = historical_memberships.unwrap_or(false);
        let mut conditions = Condition::all()
            .add_opt(restricted_chat_ids, |ids| chat::Column::Id.is_in(ids))
            .add_opt(only_active, |active| chat::Column::IsActive.eq(active))
            .add_opt(restricted_user_ids, |ids| {
                let member_subquery = Query::select()
                    .column(chat_member::Column::ChatHistoryId)
                    .from(chat_member::Entity)
                    .and_where(chat_member::Column::UserId.is_in(ids))
                    .to_owned();

                let mut member_expr =
                    Expr::col(chat_history::Column::Id).in_subquery(member_subquery);

                if !is_historical {
                    let latest_history_subquery = Query::select()
                        .expr(Expr::col(chat_history::Column::CreatedAt).max())
                        .from(chat_history::Entity)
                        .and_where(Expr::col(chat_history::Column::ChatId).equals(chat::Column::Id))
                        .to_owned();

                    member_expr = member_expr.and(
                        Expr::col(chat_history::Column::CreatedAt)
                            .in_subquery(latest_history_subquery),
                    );
                }

                member_expr
            });

        if let Some(search_phrase) = filter.search_text {
            let text_condition = apply_string_filter(
                chat_history::Column::Name,
                search_phrase,
                filter.search_operator,
            );

            let latest_history_subquery = Query::select()
                .expr(Expr::col(chat_history::Column::CreatedAt).max())
                .from(chat_history::Entity)
                .and_where(Expr::col(chat_history::Column::ChatId).equals(chat::Column::Id))
                .to_owned();

            let is_latest_condition =
                Expr::col(chat_history::Column::CreatedAt).in_subquery(latest_history_subquery);

            conditions = conditions
                .add(text_condition)
                .add_if(filter.search_in_latest, is_latest_condition);
        }

        let sort_column = match filter.sort_by.as_ref().unwrap_or(&SortBy::CreatedAt) {
            SortBy::CreatedAt => chat::Column::CreatedAt,
            SortBy::UpdatedAt => chat::Column::UpdatedAt,
            SortBy::ChatId => chat::Column::Id,
            SortBy::IsActive => chat::Column::IsActive,
        };

        let sort_order = match filter.sort_dir.unwrap_or(SortDir::Desc) {
            SortDir::Asc => Order::Asc,
            SortDir::Desc => Order::Desc,
        };

        chat::Entity::find()
            .filter(conditions)
            .order_by(sort_column, sort_order)
            .left_join(chat_history::Entity)
    }

    pub async fn assemble_chats(
        &self,
        chats: Vec<chat::Model>,
        histories: Vec<Vec<chat_history::Model>>,
    ) -> Result<Vec<Chat>, DbErr> {
        let history_ids: Vec<Uuid> = histories.iter().flatten().map(|h| h.id).collect();
        let all_chat_members = if history_ids.is_empty() {
            Vec::new()
        } else {
            chat_member::Entity::find()
                .filter(chat_member::Column::ChatHistoryId.is_in(history_ids))
                .all(self.db.as_ref())
                .await?
        };

        let mut members_by_histories_ids: HashMap<Uuid, Vec<chat_member::Model>> = HashMap::new();
        for member in all_chat_members {
            members_by_histories_ids
                .entry(member.chat_history_id)
                .or_default()
                .push(member);
        }

        let mut histories_by_chat_id: HashMap<
            Uuid,
            Vec<(chat_history::Model, Vec<chat_member::Model>)>,
        > = HashMap::new();

        for history in histories.iter().flatten() {
            let chat_id = history.chat_id;

            let members = members_by_histories_ids
                .remove(&history.id)
                .unwrap_or_default();

            histories_by_chat_id
                .entry(chat_id)
                .or_default()
                .push((history.clone(), members));
        }

        let mut assembled_chats: Vec<Chat> = Vec::new();

        for chat in chats {
            let chat_id = chat.id;
            let chat_histories = histories_by_chat_id.remove(&chat_id).unwrap_or_default();

            let transferred_chat = Chat::from(chat, chat_histories).map_err(|e| {
                DbErr::Custom(format!(
                    "Failed to transfer chat into tree structure {:?}",
                    e
                ))
            })?;

            assembled_chats.push(transferred_chat);
        }

        Ok(assembled_chats)
    }

    pub async fn list(
        &self,
        filter: ListChatsFilter,
        pagination_meta: PaginationMeta,
        restricted_chat_ids: Option<Vec<Uuid>>,
        restricted_user_ids: Option<Vec<Uuid>>,
        historical_memberships: Option<bool>,
        only_active: Option<bool>,
    ) -> Result<Vec<Chat>, DbErr> {
        let base_query = self.build_base_query(
            filter,
            restricted_chat_ids,
            restricted_user_ids,
            historical_memberships,
            only_active,
        );

        let chats = base_query
            .paginate(self.db.as_ref(), pagination_meta.per_page)
            .fetch_page(pagination_meta.current_page)
            .await?;

        let histories = chats
            .load_many(chat_history::Entity, self.db.as_ref())
            .await?;

        self.assemble_chats(chats, histories).await
    }

    pub async fn get(&self, id: Uuid) -> Result<Chat, DbErr> {
        let persited_chat = chat::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Chat with such ID has not been found".to_string())
            })?;

        let histories = chat_history::Entity::find()
            .filter(chat_history::Column::Id.eq(persited_chat.id))
            .all(self.db.as_ref())
            .await?;

        let mut assembled = self
            .assemble_chats(vec![persited_chat], vec![histories])
            .await?;

        assembled
            .pop()
            .ok_or_else(|| DbErr::Custom("Failed to map single message".to_string()))
    }
}
