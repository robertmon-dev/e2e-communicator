use sea_orm::DbErr;
use serde_json::Value;

use common::identity::{KEY_LEN, fixed, safety_number};

use crate::adapters::r#in::ipc::facade::params::{
    ChatsCreateGroupParams, InvitationsAcceptParams, InvitationsCancelParams,
    InvitationsCreateParams, InvitationsListParams, InvitationsRejectParams,
    UsersSafetyNumberParams, UsersVerifyParams,
};
use crate::adapters::r#in::ipc::facade::utils::{CODE_WRONG_STATE, FacadeError};
use crate::adapters::r#in::ipc::protocol::RpcError;
use crate::adapters::out::keys::UserIdentityStoreRepository;
use crate::domain::dto::errors::InviteError;
use crate::domain::dto::invitation::{InvitationDirection, InvitationStatus};

use super::{Facade, RpcHandler};

impl RpcHandler for InvitationsCreateParams {
    type Output = Value;
    type Error = InviteError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let invitation = facade
            .inviter
            .invite(&self.code, &self.label, self.chat_id)
            .await?;

        Ok(serde_json::to_value(invitation).expect("Invitation always serializes"))
    }
}

impl RpcHandler for InvitationsListParams {
    type Output = Value;
    type Error = FacadeError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let status = match self.status {
            Some(raw) => Some(InvitationStatus::try_from(i32::from(raw)).map_err(|_| {
                FacadeError::from(RpcError {
                    code: -32602,
                    message: "invalid invitation status".to_string(),
                })
            })?),
            None => None,
        };

        let direction = match self.direction {
            Some(raw) => Some(InvitationDirection::try_from(i32::from(raw)).map_err(|_| {
                FacadeError::from(RpcError {
                    code: -32602,
                    message: "invalid invitation direction".to_string(),
                })
            })?),
            None => None,
        };

        let invitations = facade.invitation_repository.list(status, direction).await?;

        Ok(serde_json::to_value(invitations).expect("Invitations always serialize"))
    }
}

impl RpcHandler for InvitationsAcceptParams {
    type Output = Value;
    type Error = InviteError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let invitation = facade.inviter.accept(self.invitation_id).await?;
        Ok(serde_json::to_value(invitation).expect("Invitation always serializes"))
    }
}

impl RpcHandler for InvitationsRejectParams {
    type Output = Value;
    type Error = InviteError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let invitation = facade.inviter.reject(self.invitation_id).await?;
        Ok(serde_json::to_value(invitation).expect("Invitation always serializes"))
    }
}

impl RpcHandler for InvitationsCancelParams {
    type Output = Value;
    type Error = InviteError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let invitation = facade.inviter.cancel(self.invitation_id).await?;
        Ok(serde_json::to_value(invitation).expect("Invitation always serializes"))
    }
}

impl RpcHandler for ChatsCreateGroupParams {
    type Output = Value;
    type Error = InviteError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let chat = facade.inviter.create_group(&self.name).await?;
        Ok(serde_json::to_value(chat).expect("Chat always serializes"))
    }
}

impl RpcHandler for UsersSafetyNumberParams {
    type Output = Value;
    type Error = FacadeError;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        let local = facade.require_identity()?;

        let pinned = facade
            .peer_device_repository
            .pinned_identity(self.user_id)
            .await?
            .ok_or_else(|| {
                FacadeError::from(RpcError {
                    code: CODE_WRONG_STATE,
                    message: "contact is not pinned yet".to_string(),
                })
            })?;

        let pinned_peer_key: [u8; KEY_LEN] = fixed(&pinned).map_err(|_| {
            FacadeError::from(RpcError {
                code: -32000,
                message: "pinned identity key is corrupted".to_string(),
            })
        })?;

        let own_user_key = facade
            .user_identity
            .get_or_create(local.user_id)
            .await
            .map_err(|e| {
                FacadeError::from(RpcError {
                    code: -32000,
                    message: format!("failed to load own identity key: {e}"),
                })
            })?;

        let verified_at = facade
            .peer_device_repository
            .verified_at(self.user_id)
            .await?;

        Ok(serde_json::json!({
            "user_id": self.user_id,
            "safety_number": safety_number(&own_user_key, &pinned_peer_key),
            "verified_at": verified_at,
        }))
    }
}

impl RpcHandler for UsersVerifyParams {
    type Output = Value;
    type Error = DbErr;

    async fn handle(self, facade: &Facade) -> Result<Self::Output, Self::Error> {
        facade
            .peer_device_repository
            .mark_verified(self.user_id)
            .await?;
        let verified_at = facade
            .peer_device_repository
            .verified_at(self.user_id)
            .await?;

        Ok(serde_json::json!({
            "user_id": self.user_id,
            "verified_at": verified_at,
        }))
    }
}
