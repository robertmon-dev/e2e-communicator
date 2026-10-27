use std::sync::Arc;

use async_trait::async_trait;
use tonic::transport::Channel;
use uuid::Uuid;

use common::identity::{DevicePublicKeys, KEY_LEN, SIGNATURE_LEN, fixed};

use crate::adapters::r#in::web::grpc::proto::identity as proto;
use crate::adapters::r#in::web::grpc::proto::identity::auth_client::AuthClient;
use crate::adapters::r#in::web::grpc::proto::identity::register_client::RegisterClient;
use crate::adapters::r#in::web::grpc::utils::{authorized, transport_error};
use crate::domain::dto::errors::TransportError;
use crate::domain::dto::identity::{IssuedLinkToken, RegisteredDevice, UserDirectory};
use crate::domain::services::authenticator::IdentityTransport;
use crate::domain::state::session::{SessionState, SessionToken};

pub struct GrpcIdentityClient {
    register: RegisterClient<Channel>,
    auth: AuthClient<Channel>,
    session: Arc<SessionState>,
}

impl GrpcIdentityClient {
    pub async fn connect(
        endpoint: String,
        session: Arc<SessionState>,
    ) -> Result<Self, TransportError> {
        let channel = Channel::from_shared(endpoint)
            .map_err(|e| TransportError::SendFailed(e.to_string()))?
            .connect()
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;

        Ok(Self::with_channel(channel, session))
    }

    pub fn with_channel(channel: Channel, session: Arc<SessionState>) -> Self {
        Self {
            register: RegisterClient::new(channel.clone()),
            auth: AuthClient::new(channel),
            session,
        }
    }
}

fn keys_to_proto(keys: &DevicePublicKeys) -> proto::DeviceKeys {
    proto::DeviceKeys {
        signing_key: keys.signing_key.to_vec(),
        agreement_key: keys.agreement_key.to_vec(),
    }
}

fn timestamp(unix_ms: i64) -> Result<chrono::DateTime<chrono::Utc>, TransportError> {
    chrono::DateTime::from_timestamp_millis(unix_ms)
        .ok_or_else(|| TransportError::ReceiveFailed("timestamp out of range".to_string()))
}

fn device_from_proto(device: proto::RegisteredDevice) -> Result<RegisteredDevice, TransportError> {
    let keys = device
        .keys
        .ok_or_else(|| TransportError::ReceiveFailed("device without keys".to_string()))?;
    Ok(RegisteredDevice {
        device_id: Uuid::parse_str(&device.device_id)
            .map_err(|e| TransportError::ReceiveFailed(e.to_string()))?,
        keys: DevicePublicKeys::from_slices(&keys.signing_key, &keys.agreement_key)
            .map_err(|e| TransportError::ReceiveFailed(e.to_string()))?,
        registered_at: timestamp(device.registered_at_unix_ms)?,
        cert: fixed::<SIGNATURE_LEN>(&device.cert)
            .map_err(|e| TransportError::ReceiveFailed(e.to_string()))?,
    })
}

#[async_trait]
impl IdentityTransport for GrpcIdentityClient {
    async fn register_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
        user_signing_key: &[u8; KEY_LEN],
        device_cert: &[u8; SIGNATURE_LEN],
    ) -> Result<(), TransportError> {
        let mut client = self.register.clone();
        client
            .register_device(proto::RegisterDeviceRequest {
                user_id: user_id.to_string(),
                device_id: device_id.to_string(),
                keys: Some(keys_to_proto(keys)),
                self_proof: self_proof.to_vec(),
                user_signing_key: user_signing_key.to_vec(),
                device_cert: device_cert.to_vec(),
            })
            .await
            .map_err(transport_error)?;
        Ok(())
    }

    async fn create_link_token(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<IssuedLinkToken, TransportError> {
        let mut client = self.register.clone();
        let response = client
            .create_link_token(authorized(
                &self.session,
                proto::CreateLinkTokenRequest {
                    user_id: user_id.to_string(),
                    device_id: device_id.to_string(),
                },
            ))
            .await
            .map_err(transport_error)?
            .into_inner();

        Ok(IssuedLinkToken {
            token: response.token,
            expires_at: timestamp(response.expires_at_unix_ms)?,
        })
    }

    async fn link_device(
        &self,
        token: &str,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
        user_signing_key: &[u8; KEY_LEN],
        device_cert: &[u8; SIGNATURE_LEN],
    ) -> Result<Uuid, TransportError> {
        let mut client = self.register.clone();
        let response = client
            .link_device(proto::LinkDeviceRequest {
                token: token.to_string(),
                device_id: device_id.to_string(),
                keys: Some(keys_to_proto(keys)),
                self_proof: self_proof.to_vec(),
                user_signing_key: user_signing_key.to_vec(),
                device_cert: device_cert.to_vec(),
            })
            .await
            .map_err(transport_error)?
            .into_inner();

        Uuid::parse_str(&response.user_id).map_err(|e| TransportError::ReceiveFailed(e.to_string()))
    }

    async fn revoke_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        target_device_id: Uuid,
    ) -> Result<(), TransportError> {
        let mut client = self.register.clone();
        client
            .revoke_device(authorized(
                &self.session,
                proto::RevokeDeviceRequest {
                    user_id: user_id.to_string(),
                    device_id: device_id.to_string(),
                    target_device_id: target_device_id.to_string(),
                },
            ))
            .await
            .map_err(transport_error)?;
        Ok(())
    }

    async fn lookup_devices(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<UserDirectory, TransportError> {
        let mut client = self.register.clone();
        let response = client
            .lookup_devices(authorized(
                &self.session,
                proto::LookupDevicesRequest {
                    user_id: user_id.to_string(),
                    device_id: device_id.to_string(),
                    target_user_id: target_user_id.to_string(),
                },
            ))
            .await
            .map_err(transport_error)?
            .into_inner();

        let devices = response
            .devices
            .into_iter()
            .map(device_from_proto)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(UserDirectory {
            user_signing_key: fixed::<KEY_LEN>(&response.user_signing_key)
                .map_err(|e| TransportError::ReceiveFailed(e.to_string()))?,
            devices,
        })
    }

    async fn challenge(&self, user_id: Uuid, device_id: Uuid) -> Result<[u8; 32], TransportError> {
        let mut client = self.auth.clone();
        let response = client
            .challenge(proto::ChallengeRequest {
                user_id: user_id.to_string(),
                device_id: device_id.to_string(),
            })
            .await
            .map_err(transport_error)?
            .into_inner();

        response
            .nonce
            .try_into()
            .map_err(|_| TransportError::ReceiveFailed("nonce must be 32 bytes".to_string()))
    }

    async fn authenticate(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        signature: &[u8; SIGNATURE_LEN],
    ) -> Result<SessionToken, TransportError> {
        let mut client = self.auth.clone();
        let response = client
            .authenticate(proto::AuthenticateRequest {
                user_id: user_id.to_string(),
                device_id: device_id.to_string(),
                signature: signature.to_vec(),
            })
            .await
            .map_err(transport_error)?
            .into_inner();

        Ok(SessionToken {
            token: response.token,
            expires_at: timestamp(response.expires_at_unix_ms)?,
        })
    }
}
