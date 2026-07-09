use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, Select,
};
use uuid::Uuid;

use std::sync::Arc;

use crate::client::adapters::out::persistance::entities::{chat, chat_history, chat_member};
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
        only_active: Option<bool>,
    ) -> Select<chat::Entity> {
        let mut conditions = Condition::all()
            .add_opt(restricted_chat_ids, |ids| chat::Column::Id.is_in(ids))
            .add_opt(only_active, |active| chat::Column::IsActive.eq(active))
            .add_opt(restricted_user_ids, |ids| {
                let member_subquery = Query::select()
                    .column(chat_member::Column::ChatHistoryId)
                    .from(chat_member::Entity)
                    .and_where(chat_member::Column::UserId.is_in(ids))
                    .to_owned();

                Expr::col(chat_history::Column::Id).in_subquery(member_subquery)
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
}
