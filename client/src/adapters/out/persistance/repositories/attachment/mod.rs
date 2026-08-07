pub mod create;
pub mod remove;
pub mod update;
pub mod utils;

use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait, LoaderTrait,
    PaginatorTrait, QueryFilter, Select,
};

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::utils::{ConditionExt, apply_string_filter};

use crate::client::adapters::out::persistance::entities::{
    attachment, file_metadata, message_history,
};
use crate::client::adapters::out::persistance::repositories::attachment::utils::AttachmentRestrictions;
use crate::client::domain::dto::filter_attachments::ListAttachmentsFilter;
use crate::client::domain::dto::message::Attachment;
use crate::client::domain::dto::utils::PaginationMeta;

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
            })
            .add_opt(filter.mime_type, |mime_type| {
                file_metadata::Column::MimeType.eq(mime_type)
            })
            .add_opt(filter.file_size_bytes, |file_size| {
                file_metadata::Column::FileSizeBytes.eq(file_size)
            })
            .add_opt(filter.attachment_type, |attachment_type| {
                attachment::Column::AttachmentType.eq(attachment_type as i32)
            })
            .add_opt(filter.width, |width| file_metadata::Column::Width.eq(width))
            .add_opt(filter.height, |height| {
                file_metadata::Column::Height.eq(height)
            })
            .add_opt(filter.duration_secs, |duration_secs| {
                file_metadata::Column::DurationSecs.eq(duration_secs)
            })
            .add_opt(filter.left_views, |left_views| {
                attachment::Column::ViewsLeft.eq(left_views)
            })
            .add_opt(filter.limited_view, |_| {
                attachment::Column::ViewsLeft.is_not_null()
            });

        if let Some(ids) = restricted_message_ids {
            let message_history_subquery = Query::select()
                .expr(Expr::col(message_history::Column::Id))
                .from(message_history::Entity)
                .and_where(Expr::col(message_history::Column::MessageId).is_in(ids))
                .to_owned();

            let messages_condition = Expr::col(attachment::Column::MessageHistoryId)
                .in_subquery(message_history_subquery);
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

        attachment::Entity::find()
            .filter(conditions)
            .left_join(file_metadata::Entity)
    }

    fn assemble_attachments(
        &self,
        attachments: Vec<attachment::Model>,
        file_metadatas: Vec<file_metadata::Model>,
    ) -> Result<Vec<Attachment>, DbErr> {
        let metadata_by_id: HashMap<_, _> = file_metadatas
            .into_iter()
            .map(|meta| (meta.id, meta))
            .collect();

        attachments
        .into_iter()
        .map(|attachment_model| {
            let attachment_id = attachment_model.id;

            let attachment_meta_data_model = metadata_by_id
                .get(&attachment_model.file_metadata_id)
                .cloned()
                .ok_or_else(|| {
                    DbErr::Custom(format!(
                        "Attachment with ID: {} has no file metadata",
                        attachment_id
                    ))
                })?;

            Attachment::from(attachment_model, attachment_meta_data_model).map_err(|e| {
                DbErr::Custom(format!(
                    "Failed to structurize attachment DTO for attachment with ID: {}. Original error: {}",
                    attachment_id, e
                ))
            })
        })
        .collect()
    }

    pub async fn list(
        &self,
        filter: ListAttachmentsFilter,
        pagination_meta: PaginationMeta,
        restrictions: AttachmentRestrictions,
    ) -> Result<Vec<Attachment>, DbErr> {
        let base_query = self.build_base_query(
            filter,
            restrictions.restricted_chat_ids,
            restrictions.restricted_user_ids,
            restrictions.restricted_message_ids,
            restrictions.restricted_message_history_ids,
            restrictions.restricted_metadata_ids,
        );

        let attachments = base_query
            .paginate(self.db.as_ref(), pagination_meta.per_page)
            .fetch_page(pagination_meta.current_page)
            .await?;

        let file_metadatas = attachments
            .load_many(file_metadata::Entity, self.db.as_ref())
            .await?
            .into_iter()
            .flatten()
            .collect();

        self.assemble_attachments(attachments, file_metadatas)
    }

    pub async fn get_attachment(&self, attachment_id: Uuid) -> Result<Attachment, DbErr> {
        let attachment = attachment::Entity::find_by_id(attachment_id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound(format!(
                    "No attachment found with such ID: {}",
                    attachment_id
                ))
            })?;

        let file_metadata = file_metadata::Entity::find_by_id(attachment.file_metadata_id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound(format!(
                    "File metadata with ID: {} has not been found",
                    attachment.file_metadata_id
                ))
            })?;

        Attachment::from(attachment, file_metadata).map_err(|e| {
            DbErr::Custom(format!(
                "Failed to structurize attachment DTO for attachment with ID: {}. Original error: {}",
                attachment_id, e
            ))
        })
    }
}
