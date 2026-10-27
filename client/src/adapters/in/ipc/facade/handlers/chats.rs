use sea_orm::DbErr;
use serde_json::Value;

use crate::adapters::r#in::ipc::facade::params::{
    ChatsDeleteHistoryRecord, ChatsDeleteManyParams, ChatsGetParams, ChatsListParams,
};
use crate::adapters::out::persistance::repositories::chat::{
    remove::Remove as ChatRemove, utils::ChatRestrictions,
};

use super::{Facade, RpcHandler};

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
