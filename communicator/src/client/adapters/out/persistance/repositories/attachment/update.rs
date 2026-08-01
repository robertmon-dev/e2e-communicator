use sea_orm::{DbErr, EntityTrait, IntoActiveModel, Set};

use crate::client::adapters::out::persistance::repositories::attachment::utils::{
    AttachmentUpdateInput, FileMetadataUpdateInput,
};
use crate::client::{
    adapters::out::persistance::{
        entities::attachment,
        repositories::attachment::{AttachmentRepository, update},
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

        self.get_attachment(persisted_attachment.id).await
    }

    async fn update_file_metadata(
        &self,
        update_input: FileMetadataUpdateInput,
    ) -> Result<FileMetadata, DbErr> {
        let persisted_file_metadata = attachment::Entity::find_by_id(update_input.id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("No file_metadata persisted with such ID".to_string())
            })?;

        let file_metadata_active = persisted_file_metadata.into_active_model();

        Err(DbErr::UnpackInsertId)
    }
}
