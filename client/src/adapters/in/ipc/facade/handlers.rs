use crate::{
    adapters::{
        r#in::ipc::{
            facade::{
                params::{
                    AttachmentDeleteManyParams, AttachmentDeleteParams, AttachmentGetParams,
                    AttachmentListParams, ChatsDeleteHistoryRecord, ChatsDeleteManyParams,
                    ChatsGetParams, ChatsListParams, MessagesDeleteParams, MessagesGetParams,
                    MessagesListParams, MessagesMarkDeliveredParams, SendMessageParams,
                    UsersDeleteParams,
                },
                utils::{IntoOutcome, ok, parse},
            },
            protocol::Outcome,
        },
        out::persistance::repositories::{
            attachment::remove::Remove as AttachmentRemove,
            chat::{remove::Remove as ChatRemove, utils::ChatRestrictions},
            user::remove::Remove,
        },
    },
    domain::dto::errors::MessengerError,
};
use sea_orm::DbErr;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Facade;

pub trait RpcHandler: for<'de> Deserialize<'de> {
    type Output: Serialize;
    type Error: IntoOutcome;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error>;
}

pub async fn execute_rpc<H: RpcHandler>(facade: &Facade, params: Value) -> Outcome {
    let parsed: H = match parse(params) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match parsed.handle(facade).await {
        Ok(val) => ok(serde_json::to_value(val).expect("Serialization failed")),
        Err(err) => err.into_outcome(),
    }
}

impl RpcHandler for ChatsListParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let restrictions = ChatRestrictions {
            restricted_chat_ids: None,
            restricted_user_ids: None,
            historical_memberships: None,
            only_active: Some(true),
        };
        let chats = facade
            .chat_repository
            .list(self.filter, self.pagination, restrictions)
            .await?;

        Ok(serde_json::to_value(chats).expect("Chat always serializes"))
    }
}

impl RpcHandler for ChatsGetParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let chat = facade.chat_repository.get(self.chat_id).await?;
        Ok(serde_json::to_value(chat).expect("Chat always serializes"))
    }
}

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
    type Error = MessengerError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let (message, _envelopes) = facade
            .messenger
            .send(
                self.chat_id,
                facade.user_id,
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
    type Error = MessengerError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        facade
            .messenger
            .mark_delivered(self.message_id, facade.user_id)
            .await?;

        Ok(serde_json::json!({ "success": true }))
    }
}

impl RpcHandler for MessagesDeleteParams {
    type Output = Value;
    type Error = MessengerError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        facade
            .messenger
            .delete(self.message_id, facade.user_id)
            .await?;

        Ok(serde_json::json!({ "success": true }))
    }
}

impl RpcHandler for ChatsDeleteManyParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        facade.chat_repository.remove_chats(self.chat_ids).await?;

        Ok(serde_json::json!({"success": true }))
    }
}

impl RpcHandler for ChatsDeleteHistoryRecord {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        facade
            .chat_repository
            .remove_chat_history_record(self.history_record_id)
            .await?;

        Ok(serde_json::json!({ "success": true }))
    }
}

impl RpcHandler for UsersDeleteParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        facade.user_repository.remove_user(self.user_id).await?;

        Ok(serde_json::json!({ "success": true }))
    }
}
