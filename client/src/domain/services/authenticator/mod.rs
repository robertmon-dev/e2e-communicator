use async_trait::async_trait;
use uuid::Uuid;

use common::identity::{DevicePublicKeys, SIGNATURE_LEN};

use crate::domain::dto::errors::TransportError;
use crate::domain::dto::identity::{LinkToken, RegisteredDevice};
use crate::domain::state::session::SessionToken;

#[async_trait]
pub trait IdentityTransport: Send + Sync {
    async fn register_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
    ) -> Result<(), TransportError>;

    async fn create_link_token(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<LinkToken, TransportError>;

    async fn link_device(
        &self,
        token: &str,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
    ) -> Result<Uuid, TransportError>;

    async fn revoke_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        target_device_id: Uuid,
    ) -> Result<(), TransportError>;

    async fn lookup_devices(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<Vec<RegisteredDevice>, TransportError>;

    async fn challenge(&self, user_id: Uuid, device_id: Uuid) -> Result<[u8; 32], TransportError>;

    async fn authenticate(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        signature: &[u8; SIGNATURE_LEN],
    ) -> Result<SessionToken, TransportError>;
}
