use sea_orm::{DbErr, EntityTrait, IntoActiveModel};

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

        let attachment_active = persisted_attachment.into_active_model();

        Err(DbErr::UnpackInsertId)
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
