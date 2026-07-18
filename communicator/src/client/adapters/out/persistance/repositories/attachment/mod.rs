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

use crate::client::adapters::out::persistance::entities::{attachment, file_metadata, message};
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
                .to_owned();

            let messages_condition = Expr::col(message::Column::Id).in_subquery(messages_subquery);
            conditions = conditions.add(messages_condition);
        }

        attachment::Entity::find().filter(conditions)
    }
}
