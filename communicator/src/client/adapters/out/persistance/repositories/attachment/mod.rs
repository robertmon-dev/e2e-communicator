pub mod remove;
pub mod update;

use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, LoaderTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, Select,
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::utils::{ConditionExt, apply_string_filter};

use crate::client::adapters::out::persistance::entities::{
    attachment, file_metadata, message, message_history,
};
use crate::client::domain::dto::filter_attachments::ListAttachmentsFilter;

pub struct AttachmentRepository {
    pub db: Arc<DatabaseConnection>,
}

impl AttachmentRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub fn build_base_query(
        &self,
        filter: ListAttachmentsFilter,
        restricted_chat_ids: Option<Vec<Uuid>>,
        restricted_user_ids: Option<Vec<Uuid>>,
        restricted_message_ids: Option<Vec<Uuid>>,
        restricted_message_history_ids: Option<Vec<Uuid>>,
        restricted_metadata_ids: Option<Vec<Uuid>>,
    ) -> Select<attachment::Entity> {
        let mut conditions = Condition::all()
            .add_opt(restricted_chat_ids, |ids| {
                attachment::Column::ChatId.is_in(ids)
            })
            .add_opt(restricted_user_ids, |ids| {
                attachment::Column::AuthorId.is_in(ids)
            })
            .add_opt(restricted_message_history_ids, |ids| {
                attachment::Column::MessageHistoryId.is_in(ids)
            })
            .add_opt(restricted_metadata_ids, |ids| {
                attachment::Column::FileMetadataId.is_in(ids)
            });

        if let Some(ids) = restricted_message_ids {
            let messages_subquery = Query::select()
                .expr(Expr::col(message::Column::Id))
                .from(message::Entity)
                .and_where(Expr::col(message::Column::Id).is_in(ids))
                .and_where(
                    Expr::col(message_history::Column::MessageId).equals(message::Column::Id),
                )
                .and_where(
                    Expr::col(message_history::Column::Id)
                        .equals(attachment::Column::MessageHistoryId),
                )
                .to_owned();

            let messages_condition = Expr::col(message::Column::Id).in_subquery(messages_subquery);
            conditions = conditions.add(messages_condition);
        }

        if let Some(search_phrase) = filter.file_name {
            let text_condition = apply_string_filter(
                file_metadata::Column::FileName,
                search_phrase,
                filter.search_operator,
            );

            let latest_attachment_subquery = Query::select()
                .expr(Expr::col(attachment::Column::CreatedAt).max())
                .from(attachment::Entity)
                .to_owned();

            let is_latest_condition =
                Expr::col(attachment::Column::CreatedAt).in_subquery(latest_attachment_subquery);

            conditions = conditions
                .add(text_condition)
                .add_if(filter.search_in_latest, is_latest_condition);
        }

        if let Some(file_size) = filter.file_size_bytes {
            let file_size_subquery = Query::select()
                .expr(Expr::col(file_metadata::Column::FileSizeBytes).eq(file_size))
                .from(file_metadata::Entity)
                .to_owned();

            let file_size_condition =
                Expr::col(file_metadata::Column::FileSizeBytes).in_subquery(file_size_subquery);

            conditions = conditions.add(file_size_condition);
        }

        attachment::Entity::find()
            .filter(conditions)
            .left_join(file_metadata::Entity)
    }
}
