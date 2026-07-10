pub mod remove;
pub mod update;

use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, LoaderTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select,
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::{
    attachment, file_metadata, message, message_history,
};
use crate::client::domain::dto::filter_messages::{ListMessagesFilter, SortBy};
use crate::client::domain::dto::message::Message;
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

    pub async fn get(&self, id: Uuid) -> Result<Message, DbErr> {
        let message = message::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| DbErr::RecordNotFound(format!("Message with ID {} not found", id)))?;

        let histories = message_history::Entity::find()
            .filter(message_history::Column::MessageId.eq(message.id))
            .all(self.db.as_ref())
            .await?;

        let history_ids: Vec<Uuid> = histories.iter().map(|h| h.id).collect();
        let all_attachments = if history_ids.is_empty() {
            Vec::new()
        } else {
            attachment::Entity::find()
                .filter(attachment::Column::MessageHistoryId.is_in(history_ids))
                .all(self.db.as_ref())
                .await?
        };

        let file_metadata_ids: Vec<Uuid> =
            all_attachments.iter().map(|a| a.file_metadata_id).collect();
        let all_metadatas = if file_metadata_ids.is_empty() {
            Vec::new()
        } else {
            file_metadata::Entity::find()
                .filter(file_metadata::Column::Id.is_in(file_metadata_ids))
                .all(self.db.as_ref())
                .await?
        };

        let metadata_by_id: HashMap<Uuid, file_metadata::Model> =
            all_metadatas.into_iter().map(|m| (m.id, m)).collect();

        let mut attachments_with_meta_by_history: HashMap<
            Uuid,
            Vec<(attachment::Model, file_metadata::Model)>,
        > = HashMap::new();

        for att in all_attachments {
            if let Some(meta) = metadata_by_id.get(&att.file_metadata_id).cloned() {
                attachments_with_meta_by_history
                    .entry(att.message_history_id)
                    .or_default()
                    .push((att, meta));
            }
        }

        let mut history_payload = Vec::with_capacity(histories.len());

        for history in histories {
            let mut history_atts = Vec::new();
            let mut history_metas = Vec::new();

            if let Some(atts_with_meta) = attachments_with_meta_by_history.remove(&history.id) {
                for (att, meta) in atts_with_meta {
                    history_atts.push(att);
                    history_metas.push(meta);
                }
            }

            history_payload.push((history, (history_atts, history_metas)));
        }

        Message::from(message, history_payload)
            .map_err(|e| DbErr::Custom(format!("Domain mapping failed: {:?}", e)))
    }

    pub async fn list(
        &self,
        filter: ListMessagesFilter,
        pagination_meta: PaginationMeta,
        restricted_chat_ids: Option<Vec<Uuid>>,
        restricted_author_ids: Option<Vec<Uuid>>,
        restricted_reply_to_ids: Option<Vec<Uuid>>,
        only_replies: bool,
    ) -> Result<Vec<Message>, DbErr> {
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

        let history_ids: Vec<Uuid> = histories.iter().flatten().map(|h| h.id).collect();
        let all_attachments = if history_ids.is_empty() {
            Vec::new()
        } else {
            attachment::Entity::find()
                .filter(attachment::Column::MessageHistoryId.is_in(history_ids))
                .all(self.db.as_ref())
                .await?
        };

        let file_metadata_ids: Vec<Uuid> =
            all_attachments.iter().map(|a| a.file_metadata_id).collect();

        let all_metadatas = if file_metadata_ids.is_empty() {
            Vec::new()
        } else {
            file_metadata::Entity::find()
                .filter(file_metadata::Column::Id.is_in(file_metadata_ids))
                .all(self.db.as_ref())
                .await?
        };

        let metadata_by_id: HashMap<Uuid, file_metadata::Model> =
            all_metadatas.into_iter().map(|m| (m.id, m)).collect();

        let mut attachments_with_meta_by_history: HashMap<
            Uuid,
            Vec<(attachment::Model, file_metadata::Model)>,
        > = HashMap::new();

        for att in all_attachments {
            if let Some(meta) = metadata_by_id.get(&att.file_metadata_id).cloned() {
                attachments_with_meta_by_history
                    .entry(att.message_history_id)
                    .or_default()
                    .push((att, meta));
            }
        }

        let mut transferred_messages = Vec::with_capacity(messages.len());

        for (message, message_histories) in messages.into_iter().zip(histories.into_iter()) {
            let mut history_payload = Vec::with_capacity(message_histories.len());

            for history in message_histories {
                let mut history_atts = Vec::new();
                let mut history_metas = Vec::new();

                if let Some(atts_with_meta) = attachments_with_meta_by_history.remove(&history.id) {
                    for (att, meta) in atts_with_meta {
                        history_atts.push(att);
                        history_metas.push(meta);
                    }
                }

                history_payload.push((history, (history_atts, history_metas)));
            }

            if let Ok(recombied) = Message::from(message, history_payload) {
                transferred_messages.push(recombied);
            }
        }

        Ok(transferred_messages)
    }
}
