use sea_orm::{ActiveModelTrait, DbErr, EntityTrait, IntoActiveModel, Set};

use crate::client::adapters::out::persistance::entities::file_metadata;
use crate::client::adapters::out::persistance::repositories::attachment::utils::{
    AttachmentUpdateInput, FileMetadataUpdateInput,
};
use crate::client::{
    adapters::out::persistance::{
        entities::attachment, repositories::attachment::AttachmentRepository,
    },
    domain::dto::{message::Attachment, utils::FileMetadata},
};

pub trait Update {
    async fn update_attachment(
        &self,
        update_input: AttachmentUpdateInput,
    ) -> Result<Attachment, DbErr>;
    async fn update_file_metadata(
        &self,
        update_input: FileMetadataUpdateInput,
    ) -> Result<FileMetadata, DbErr>;
}

impl Update for AttachmentRepository {
    async fn update_attachment(
        &self,
        update_input: AttachmentUpdateInput,
    ) -> Result<Attachment, DbErr> {
        let persisted_attachment = attachment::Entity::find_by_id(update_input.id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("No attachment persisted with such ID".to_string())
            })?;

        let mut attachment_active = persisted_attachment.clone().into_active_model();

        if let Some(file_type) = update_input.attachment_type {
            let raw_file_type = file_type.into();
            attachment_active.attachment_type = Set(raw_file_type);
        }

        if let Some(available_date) = update_input.available_to {
            attachment_active.available_to = Set(Some(available_date));
        }

        if let Some(current_views) = update_input.views {
            attachment_active.views_left = Set(Some(current_views));
        }

        attachment_active.update(self.db.as_ref()).await?;

        self.get_attachment(persisted_attachment.id).await
    }

    async fn update_file_metadata(
        &self,
        update_input: FileMetadataUpdateInput,
    ) -> Result<FileMetadata, DbErr> {
        let persisted_file_attachment = attachment::Entity::find_by_id(update_input.id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("No attachment persisted with such ID".to_string())
            })?;

        let persisted_file_metadata =
            file_metadata::Entity::find_by_id(persisted_file_attachment.file_metadata_id)
                .one(self.db.as_ref())
                .await?
                .ok_or_else(|| {
                    DbErr::RecordNotFound("No file_metadata persisted with such ID".to_string())
                })?;

        let mut file_metadata_active = persisted_file_metadata.into_active_model();

        if let Some(file_name) = update_input.file_name {
            file_metadata_active.file_name = Set(file_name);
        }

        if let Some(file_size_bytes) = update_input.file_size_bytes {
            file_metadata_active.file_size_bytes = Set(file_size_bytes);
        }

        if let Some(mime_type) = update_input.mime_type {
            file_metadata_active.mime_type = Set(Some(mime_type));
        }

        if let Some(width) = update_input.width {
            file_metadata_active.width = Set(Some(width));
        }

        if let Some(height) = update_input.height {
            file_metadata_active.height = Set(Some(height));
        }

        if let Some(duration_secs) = update_input.duration_secs {
            file_metadata_active.duration_secs = Set(Some(duration_secs));
        }

        if let Some(thumbnail_link) = update_input.thumbnail_link {
            file_metadata_active.thumbnail_link = Set(Some(thumbnail_link));
        }

        let updated_model = file_metadata_active.update(self.db.as_ref()).await?;

        Ok(FileMetadata {
            id: updated_model.id,
            file_name: updated_model.file_name,
            file_size_bytes: updated_model.file_size_bytes,
            mime_type: updated_model.mime_type,
            width: updated_model.width,
            height: updated_model.height,
            duration_secs: updated_model.duration_secs,
            thumbnail_link: updated_model.thumbnail_link,
            created_at: updated_model.created_at,
        })
    }
}
