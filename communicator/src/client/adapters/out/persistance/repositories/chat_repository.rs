use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, Select,
};
use uuid::Uuid;

use std::sync::Arc;

use crate::client::adapters::out::persistance::entities::{
    chat, chat_history, message, message_history,
};
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
        restricted_author_ids: Option<Vec<Uuid>>,
        restricted_reply_to_ids: Option<Vec<Uuid>>,
        only_replies: bool,
    ) -> Select<chat::Entity> {
        let mut conditions = Condition::all();

        chat::Entity::find()
            .filter(conditions)
            .left_join(chat_history::Entity)
    }
}
