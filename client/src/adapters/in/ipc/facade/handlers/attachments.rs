use sea_orm::DbErr;
use serde_json::Value;

use crate::adapters::r#in::ipc::facade::params::{
    AttachmentDeleteManyParams, AttachmentDeleteParams, AttachmentGetParams, AttachmentListParams,
};
use crate::adapters::out::persistance::repositories::attachment::remove::Remove as AttachmentRemove;

use super::{Facade, RpcHandler};

impl RpcHandler for AttachmentGetParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let attachment = facade
            .attachment_repository
            .get_attachment(self.attachment_id)
            .await?;

        Ok(serde_json::to_value(attachment).expect("Attachment always serializes"))
    }
}

impl RpcHandler for AttachmentListParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let attachments = facade
            .attachment_repository
            .list(self.filters, self.pagination, self.restrictions)
            .await?;

        Ok(serde_json::to_value(attachments).expect("Attachments always serialize"))
    }
}

impl RpcHandler for AttachmentDeleteParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let res = facade
            .attachment_repository
            .remove_attachments(vec![self.attachment_id])
            .await?;

        Ok(serde_json::to_value(res).expect("Attachment always serializes"))
    }
}

impl RpcHandler for AttachmentDeleteManyParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let res = facade
            .attachment_repository
            .remove_attachments(self.attachment_ids)
            .await?;

        Ok(serde_json::to_value(res).expect("Attachments always serialize"))
    }
}
