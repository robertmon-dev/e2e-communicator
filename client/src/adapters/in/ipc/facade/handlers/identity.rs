use serde_json::Value;

use common::identity::{KEY_LEN, decode_token_array};

use crate::adapters::r#in::ipc::facade::params::{DevicesRevokeParams, IdentityLinkParams};
use crate::adapters::r#in::ipc::facade::utils::FacadeError;
use crate::adapters::r#in::ipc::protocol::RpcError;

use super::{Facade, RpcHandler};

impl RpcHandler for IdentityLinkParams {
    type Output = Value;
    type Error = FacadeError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let user_seed = decode_token_array::<KEY_LEN>(&self.user_seed).map_err(|e| {
            FacadeError::from(RpcError {
                code: -32602,
                message: format!("invalid user_seed: {e}"),
            })
        })?;
        let identity = facade
            .authenticator
            .link(self.user_id, &self.token, &user_seed)
            .await?;
        Ok(serde_json::to_value(identity).expect("LocalIdentity always serializes"))
    }
}

impl RpcHandler for DevicesRevokeParams {
    type Output = Value;
    type Error = FacadeError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        facade.authenticator.revoke_device(self.device_id).await?;
        Ok(serde_json::json!({}))
    }
}
