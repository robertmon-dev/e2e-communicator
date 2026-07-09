use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, Select,
};
use uuid::Uuid;

use std::sync::Arc;

use crate::client::adapters::out::persistance::entities::{message, message_history};
use crate::client::domain::dto::filter_messages::ListMessagesFilter;
use crate::client::domain::dto::utils::{PaginationMeta, SortBy, SortDir};

use super::utils::apply_string_filter;

pub struct MessageRepository {
    db: Arc<DatabaseConnection>,
}

impl MessageRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    fn build_base_query(
        &self,
        filter: ListMessagesFilter,
        restricted_chat_ids: Option<Vec<Uuid>>,
        restricted_author_ids: Option<Vec<Uuid>>,
        restricted_reply_to_ids: Option<Vec<Uuid>>,
        only_replies: bool,
    ) -> Select<message::Entity> {
        let mut conditions = Condition::all();

        if let Some(uid) = restricted_chat_ids {
            conditions = conditions.add(message::Column::ChatId.is_in(uid));
        }

        if let Some(uid) = restricted_reply_to_ids {
            conditions = conditions.add(message::Column::ReplyToId.is_in(uid));
        }

        if let Some(uid) = restricted_author_ids {
            conditions = conditions.add(message::Column::AuthorId.is_in(uid));
        }

        if let Some(uid) = filter.chat_id {
            conditions = conditions.add(message::Column::ChatId.eq(uid));
        }

        if let Some(uid) = filter.author_id {
            conditions = conditions.add(message::Column::AuthorId.eq(uid));
        }

        if let Some(from) = filter.from {
            conditions = conditions.add(message::Column::CreatedAt.gte(from));
        }

        if let Some(to) = filter.to {
            conditions = conditions.add(message::Column::CreatedAt.lt(to));
        }

        if let Some(from) = filter.modified_from {
            conditions = conditions.add(message::Column::UpdatedAt.gte(from));
        }

        if let Some(to) = filter.modified_to {
            conditions = conditions.add(message::Column::UpdatedAt.lt(to));
        }

        if let Some(to) = filter.available_to {
            conditions = conditions.add(message::Column::AvailableTo.lte(to));
        }

        if only_replies {
            conditions = conditions.add(message::Column::IsReply.eq(true));
        }

        if let Some(search_phrase) = filter.search_text {
            let text_condition = apply_string_filter(
                message_history::Column::Text,
                search_phrase,
                filter.search_operator,
            );

            let latest_history_subquery = Query::select()
                .expr(Expr::col(message_history::Column::CreatedAt).max())
                .from(message_history::Entity)
                .and_where(
                    Expr::col(message_history::Column::MessageId).equals(message::Column::Id),
                )
                .to_owned();

            let is_latest_condition =
                Expr::col(message_history::Column::CreatedAt).in_subquery(latest_history_subquery);

            conditions = conditions.add(text_condition).add(is_latest_condition);
        }

        let sort_column = match filter.sort_by.as_ref().unwrap_or(&SortBy::CreatedAt) {
            SortBy::CreatedAt => message::Column::CreatedAt,
            SortBy::UpdatedAt => message::Column::UpdatedAt,
            SortBy::AuthorId => message::Column::AuthorId,
            SortBy::IsReply => message::Column::IsReply,
            SortBy::AvailableTo => message::Column::AvailableTo,
            SortBy::ReplyToId => message::Column::ReplyToId,
            SortBy::ChatId => message::Column::ChatId,
        };

        let sort_order = match filter.sort_dir.unwrap_or(SortDir::Desc) {
            SortDir::Asc => Order::Asc,
            SortDir::Desc => Order::Desc,
        };

        message::Entity::find()
            .filter(conditions)
            .left_join(message_history::Entity)
            .order_by(sort_column, sort_order)
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<message::Model>, DbErr> {
        message::Entity::find_by_id(id)
            .left_join(message_history::Entity)
            .one(self.db.as_ref())
            .await
    }

    pub async fn list(
        &self,
        filter: ListMessagesFilter,
        pagination_meta: PaginationMeta,
        restricted_chat_ids: Option<Vec<Uuid>>,
        restricted_author_ids: Option<Vec<Uuid>>,
        restricted_reply_to_ids: Option<Vec<Uuid>>,
        only_replies: bool,
    ) -> Result<Vec<message::Model>, DbErr> {
        let base_query = self.build_base_query(
            filter,
            restricted_chat_ids,
            restricted_author_ids,
            restricted_reply_to_ids,
            only_replies,
        );

        let messages = base_query
            .paginate(self.db.as_ref(), pagination_meta.per_page)
            .fetch_page(pagination_meta.current_page)
            .await?;

        Ok(messages)
    }
}
