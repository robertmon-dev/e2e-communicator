use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, LoaderTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, Select,
};
use uuid::Uuid;

use std::sync::Arc;

use crate::client::adapters::out::persistance::entities::{message, message_history};
use crate::client::domain::dto::filter_messages::{ListMessagesFilter, SortBy};
use crate::client::domain::dto::utils::{PaginationMeta, SortDir};

use super::utils::{ConditionExt, apply_string_filter};

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
        let mut conditions = Condition::all()
            .add_opt(restricted_chat_ids, |ids| {
                message::Column::ChatId.is_in(ids)
            })
            .add_opt(restricted_reply_to_ids, |ids| {
                message::Column::ReplyToId.is_in(ids)
            })
            .add_opt(restricted_author_ids, |ids| {
                message::Column::AuthorId.is_in(ids)
            })
            .add_opt(filter.chat_id, |id| message::Column::ChatId.eq(id))
            .add_opt(filter.author_id, |id| message::Column::AuthorId.eq(id))
            .add_opt(filter.from, |from| message::Column::CreatedAt.gte(from))
            .add_opt(filter.to, |to| message::Column::CreatedAt.lt(to))
            .add_opt(filter.available_to, |to| {
                message::Column::AvailableTo.lte(to)
            })
            .add_if(only_replies, message::Column::IsReply.eq(true));

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

            conditions = conditions
                .add(text_condition)
                .add_if(filter.search_in_latest, is_latest_condition);
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
    ) -> Result<Vec<(message::Model, Vec<message_history::Model>)>, DbErr> {
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

        let histories = messages
            .load_many(message_history::Entity, self.db.as_ref())
            .await?;

        let mut result = Vec::new();
        for (message, history) in messages.into_iter().zip(histories.into_iter()) {
            result.push((message, history));
        }

        Ok(result)
    }
}
