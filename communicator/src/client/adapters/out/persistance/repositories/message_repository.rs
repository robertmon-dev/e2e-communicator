use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, Order, QueryFilter, QueryOrder,
    Select,
};
use uuid::Uuid;

use std::sync::Arc;

use crate::client::adapters::out::persistance::entities::message;
use crate::client::domain::dto::filter_messages::{ListMessagesFilter, SortBy, SortDir};

pub struct MessageRepository {
    db: Arc<DatabaseConnection>,
}

impl MessageRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub fn build_base_query(
        &self,
        filter: ListMessagesFilter,
        restricted_chat_ids: Option<Vec<Uuid>>,
        restricted_author_ids: Option<Vec<Uuid>>,
        restricted_reply_to_ids: Option<Vec<Uuid>>,
    ) -> Select<message::Entity> {
        let mut query = message::Entity::find();
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

        query = query.filter(conditions);

        let sort_column = match filter.sort_by.as_ref().unwrap_or(&SortBy::CreatedAt) {
            SortBy::CreatedAt => message::Column::CreatedAt,
            SortBy::UpdatedAt => message::Column::UpdatedAt,
            SortBy::AuthorId => message::Column::AuthorId,
            SortBy::IsReply => message::Column::IsReply,
            SortBy::AvailableTo => message::Column::AvailableTo,
            SortBy::ReplyToId => message::Column::ReplyToId,
            SortBy::ChatId => message::Column::ChatId,
            _ => message::Column::CreatedAt,
        };

        let sort_order = match filter.sort_dir.unwrap_or(SortDir::Desc) {
            SortDir::Asc => Order::Asc,
            SortDir::Desc => Order::Desc,
        };

        query.order_by(sort_column, sort_order)
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<message::Model>, DbErr> {
        message::Entity::find_by_id(id).one(self.db.as_ref()).await
    }
}
