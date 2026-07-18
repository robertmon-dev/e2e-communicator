use sea_orm::{DbErr, IntoActiveModel, TransactionTrait};

use crate::client::adapters::out::persistance::entities::{attachment, file_metadata};
use crate::client::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use crate::client::adapters::out::persistance::repositories::utils::safe_bulk_insert;
use crate::client::domain::dto::message::Attachment;

pub trait Create {
    async fn create(&self, attachment: Attachment) -> Result<(), DbErr>;
}

impl Create for AttachmentRepository {
    async fn create(&self, attachment: Attachment) -> Result<(), DbErr> {
        let attachment_link = attachment.attachment_link.map(|url| url.to_string());
        let attachment_model = attachment::Model {
            id: attachment.id,
            author_id: attachment.author_id,
            message_history_id: attachment.message_history_id,
            chat_id: attachment.chat_id,
            file_metadata_id: attachment.file_metadata.id,
            attachment_link,
            attachment_type: attachment.attachment_type.into(),
            available_to: attachment.available_to,
            created_at: attachment.created_at,
            updated_at: attachment.updated_at,
            views_left: attachment.views_left,
        };

        let file_metadata_model = file_metadata::Model {
            id: attachment.file_metadata.id,
            file_name: attachment.file_metadata.file_name,
            file_size_bytes: attachment.file_metadata.file_size_bytes,
            width: attachment.file_metadata.width,
            height: attachment.file_metadata.height,
            duration_secs: attachment.file_metadata.duration_secs,
            mime_type: attachment.file_metadata.mime_type,
            thumbnail_link: attachment.file_metadata.thumbnail_link,
            created_at: attachment.file_metadata.created_at,
        };

        let attachment_model_active = attachment_model.into_active_model();
        let file_metadata_model_active = file_metadata_model.into_active_model();

        let txn = self.db.begin().await?;

        safe_bulk_insert::<attachment::Entity, _, _>(&txn, vec![attachment_model_active]).await?;
        safe_bulk_insert::<file_metadata::Entity, _, _>(&txn, vec![file_metadata_model_active])
            .await?;

        txn.commit().await?;

        Ok(())
    }
}
