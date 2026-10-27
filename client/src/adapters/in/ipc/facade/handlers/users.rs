use sea_orm::DbErr;
use serde_json::Value;

use crate::adapters::r#in::ipc::facade::params::{
    UsersDeleteParams, UsersGetParams, UsersListParams, UsersUpdateParams,
};
use crate::adapters::out::persistance::repositories::user::{remove::Remove, update::Update};

use super::{Facade, RpcHandler};

impl RpcHandler for UsersDeleteParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let user = facade.user_repository.remove_user(self.user_id).await?;
        Ok(serde_json::to_value(user).expect("Users always serializes"))
    }
}

impl RpcHandler for UsersListParams {
    type Error = DbErr;
    type Output = Value;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let users = facade
            .user_repository
            .list(self.pagination, self.restrictions)
            .await?;
        Ok(serde_json::to_value(users).expect("Users lsit always serializes"))
    }
}

impl RpcHandler for UsersGetParams {
    type Error = DbErr;
    type Output = Value;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let user = facade.user_repository.get(self.user_id).await?;
        Ok(serde_json::to_value(user).expect("User always serializes"))
    }
}

impl RpcHandler for UsersUpdateParams {
    type Error = DbErr;
    type Output = Value;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let user = facade.user_repository.update_user(self.input).await?;
        Ok(serde_json::to_value(user).expect("User always serializes"))
    }
}
