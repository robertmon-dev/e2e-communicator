use sea_orm::{DbErr, IntoActiveModel, TransactionTrait};

use crate::adapters::out::persistance::entities::{attachment, file_metadata};
use crate::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use crate::adapters::out::persistance::repositories::utils::{safe_bulk_insert, safe_bulk_upsert};
use crate::domain::dto::message::Attachment;

pub trait Create {
    async fn create(&self, attachment: Attachment) -> Result<(), DbErr>;
    async fn create_many(&self, attachments: Vec<Attachment>) -> Result<(), DbErr>;
    async fn upsert(&self, attachment: Attachment) -> Result<(), DbErr>;
    async fn upsert_many(&self, attachments: Vec<Attachment>) -> Result<(), DbErr>;
}

impl Create for AttachmentRepository {
    async fn upsert_many(&self, attachments: Vec<Attachment>) -> Result<(), DbErr> {
        let attachments_model_active: Vec<attachment::ActiveModel> = attachments
            .iter()
            .map(|attachment| attachment.into_model().into_active_model())
            .collect();

        let file_metadata_model_active: Vec<file_metadata::ActiveModel> = attachments
            .into_iter()
            .map(|attachment| attachment.file_metadata.into_model().into_active_model())
            .collect();

        let txn = self.db.begin().await?;

        if !file_metadata_model_active.is_empty() {
            safe_bulk_upsert::<file_metadata::Entity, _, _>(&txn, file_metadata_model_active)
                .await?;
        }

        if !attachments_model_active.is_empty() {
            safe_bulk_upsert::<attachment::Entity, _, _>(&txn, attachments_model_active).await?;
        }

        txn.commit().await?;

        Ok(())
    }

    async fn create_many(&self, attachments: Vec<Attachment>) -> Result<(), DbErr> {
        let attachments_model_active: Vec<attachment::ActiveModel> = attachments
            .iter()
            .map(|attachment| attachment.into_model().into_active_model())
            .collect();

        let file_metadata_model_active: Vec<file_metadata::ActiveModel> = attachments
            .into_iter()
            .map(|attachment| attachment.file_metadata.into_model().into_active_model())
            .collect();

        let txn = self.db.begin().await?;

        if !file_metadata_model_active.is_empty() {
            safe_bulk_insert::<file_metadata::Entity, _, _>(&txn, file_metadata_model_active)
                .await?;
        }

        if !attachments_model_active.is_empty() {
            safe_bulk_insert::<attachment::Entity, _, _>(&txn, attachments_model_active).await?;
        }

        txn.commit().await?;

        Ok(())
    }

    async fn create(&self, attachment: Attachment) -> Result<(), DbErr> {
        let attachment_model_active = attachment.into_model().into_active_model();
        let file_metadata_model_active = attachment.file_metadata.into_model().into_active_model();

        let txn = self.db.begin().await?;

        safe_bulk_insert::<file_metadata::Entity, _, _>(&txn, vec![file_metadata_model_active])
            .await?;
        safe_bulk_insert::<attachment::Entity, _, _>(&txn, vec![attachment_model_active]).await?;

        txn.commit().await?;

        Ok(())
    }

    async fn upsert(&self, attachment: Attachment) -> Result<(), DbErr> {
        let attachment_model_active = attachment.into_model().into_active_model();
        let file_metadata_model_active = attachment.file_metadata.into_model().into_active_model();

        let txn = self.db.begin().await?;

        safe_bulk_upsert::<file_metadata::Entity, _, _>(&txn, vec![file_metadata_model_active])
            .await?;
        safe_bulk_upsert::<attachment::Entity, _, _>(&txn, vec![attachment_model_active]).await?;

        txn.commit().await?;

        Ok(())
    }
}
