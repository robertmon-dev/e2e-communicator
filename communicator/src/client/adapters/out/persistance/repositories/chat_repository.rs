use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, Select,
};
use uuid::Uuid;

use std::sync::Arc;

use crate::client::adapters::out::persistance::entities::{chat, chat_history};
use crate::client::domain::dto::filter_chat::ListChatsFilter;
use crate::client::domain::dto::utils::{PaginationMeta, SortBy, SortDir};

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
        only_active: bool,
    ) -> Select<chat::Entity> {
        let mut conditions = Condition::all();

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

        chat::Entity::find()
            .filter(conditions)
            .left_join(chat_history::Entity)
    }
}
