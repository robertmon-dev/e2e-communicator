use uuid::Uuid;

use sea_orm::{ColumnTrait, DbErr, EntityTrait, QueryFilter};

use crate::adapters::out::persistance::entities::{attachment, file_metadata};
use crate::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use crate::domain::dto::message::Attachment;

pub trait Remove {
    async fn remove_attachment(&self, attachment_id: Uuid) -> Result<Attachment, DbErr>;
    async fn remove_attachments(&self, attachment_ids: Vec<Uuid>)
    -> Result<Vec<Attachment>, DbErr>;
}

impl Remove for AttachmentRepository {
    async fn remove_attachment(&self, attachment_id: Uuid) -> Result<Attachment, DbErr> {
        let attachment = self.get_attachment(attachment_id).await?;

        let delete_result = attachment::Entity::delete_by_id(attachment_id)
            .exec(self.db.as_ref())
            .await?;

        if delete_result.rows_affected == 0 {
            return Err(DbErr::RecordNotUpdated);
        }

        Ok(attachment)
    }

    async fn remove_attachments(
        &self,
        attachment_ids: Vec<Uuid>,
    ) -> Result<Vec<Attachment>, DbErr> {
        if attachment_ids.is_empty() {
            return Ok(Vec::new());
        }

        let attachments = attachment::Entity::find()
            .filter(attachment::Column::Id.is_in(attachment_ids.clone()))
            .all(self.db.as_ref())
            .await?;

        let file_metadata_ids: Vec<Uuid> = attachments.iter().map(|a| a.file_metadata_id).collect();

        let file_metadatas = file_metadata::Entity::find()
            .filter(file_metadata::Column::Id.is_in(file_metadata_ids))
            .all(self.db.as_ref())
            .await?;

        let attachments_to_remove = self.assemble_attachments(attachments, file_metadatas)?;

        attachment::Entity::delete_many()
            .filter(attachment::Column::Id.is_in(attachment_ids))
            .exec(self.db.as_ref())
            .await?;

        Ok(attachments_to_remove)
    }
}
