use sea_orm::DbErr;
use serde_json::Value;

use crate::adapters::r#in::ipc::facade::params::{
    MessagesDeleteParams, MessagesGetParams, MessagesListParams, MessagesMarkDeliveredParams,
    SendMessageParams,
};
use crate::adapters::r#in::ipc::facade::utils::FacadeError;

use super::{Facade, RpcHandler};

impl RpcHandler for MessagesListParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let messages = facade
            .message_repository
            .list(self.filter, self.pagination, None, None, None, false)
            .await?;

        Ok(serde_json::to_value(messages).expect("Messages always serialize"))
    }
}

impl RpcHandler for MessagesGetParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let message = facade.message_repository.get(self.message_id).await?;
        Ok(serde_json::to_value(message).expect("Message always serializes"))
    }
}

impl RpcHandler for SendMessageParams {
    type Output = Value;
    type Error = FacadeError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let identity = facade.require_identity()?;
        let (message, _envelopes) = facade
            .messenger
            .send(
                self.chat_id,
                identity.user_id,
                identity.device_id,
                self.text,
                self.reply_to_id,
                self.available_for,
                self.attachments,
                self.scheduled_for,
            )
            .await?;

        Ok(serde_json::to_value(message).expect("Message always serializes"))
    }
}

impl RpcHandler for MessagesMarkDeliveredParams {
    type Output = Value;
    type Error = FacadeError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let identity = facade.require_identity()?;
        facade
            .messenger
            .mark_delivered(self.message_id, identity.user_id, identity.device_id)
            .await?;

        Ok(serde_json::json!({ "success": true }))
    }
}

impl RpcHandler for MessagesDeleteParams {
    type Output = Value;
    type Error = FacadeError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let identity = facade.require_identity()?;
        facade
            .messenger
            .delete(self.message_id, identity.user_id, identity.device_id)
            .await?;

        Ok(serde_json::json!({ "success": true }))
    }
}
