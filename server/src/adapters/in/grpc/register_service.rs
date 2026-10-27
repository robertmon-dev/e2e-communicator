use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::adapters::r#in::grpc::proto::identity as proto;
use crate::adapters::r#in::grpc::proto::identity::register_server::Register;
use crate::adapters::r#in::grpc::session::{ensure_claims, resolve_session};
use crate::adapters::r#in::grpc::utils::{
    device_to_proto, parse_device_keys, parse_signature, parse_signing_key, parse_uuid,
    registry_status,
};
use crate::domain::services::mailbox::Mailbox;
use crate::domain::services::registry::Registry;

pub struct RegisterService {
    registry: Arc<Registry>,
    mailbox: Arc<Mailbox>,
}

impl RegisterService {
    pub fn new(registry: Arc<Registry>, mailbox: Arc<Mailbox>) -> Self {
        Self { registry, mailbox }
    }
}

#[tonic::async_trait]
impl Register for RegisterService {
    async fn register_device(
        &self,
        request: Request<proto::RegisterDeviceRequest>,
    ) -> Result<Response<proto::RegisterDeviceResponse>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid(&req.user_id, "user_id")?;
        let device_id = parse_uuid(&req.device_id, "device_id")?;
        let keys = parse_device_keys(req.keys)?;
        let proof = parse_signature(&req.self_proof, "self_proof")?;
        let user_signing_key = parse_signing_key(&req.user_signing_key, "user_signing_key")?;
        let device_cert = parse_signature(&req.device_cert, "device_cert")?;

        self.registry
            .register_device(
                user_id,
                device_id,
                &keys,
                &proof,
                &user_signing_key,
                &device_cert,
            )
            .await
            .map_err(registry_status)?;

        Ok(Response::new(proto::RegisterDeviceResponse {}))
    }

    async fn create_link_token(
        &self,
        request: Request<proto::CreateLinkTokenRequest>,
    ) -> Result<Response<proto::CreateLinkTokenResponse>, Status> {
        let session = resolve_session(&request, &self.registry).await?;
        let user_id = parse_uuid(&request.get_ref().user_id, "user_id")?;
        let device_id = parse_uuid(&request.get_ref().device_id, "device_id")?;
        ensure_claims(&session, user_id, device_id)?;

        let link = self
            .registry
            .create_link_token(session.user_id)
            .await
            .map_err(registry_status)?;

        Ok(Response::new(proto::CreateLinkTokenResponse {
            token: link.token,
            expires_at_unix_ms: link.expires_at.timestamp_millis(),
        }))
    }

    async fn link_device(
        &self,
        request: Request<proto::LinkDeviceRequest>,
    ) -> Result<Response<proto::LinkDeviceResponse>, Status> {
        let req = request.into_inner();
        let device_id = parse_uuid(&req.device_id, "device_id")?;
        let keys = parse_device_keys(req.keys)?;
        let proof = parse_signature(&req.self_proof, "self_proof")?;
        let user_signing_key = parse_signing_key(&req.user_signing_key, "user_signing_key")?;
        let device_cert = parse_signature(&req.device_cert, "device_cert")?;

        let user_id = self
            .registry
            .link_device(
                &req.token,
                device_id,
                &keys,
                &proof,
                &user_signing_key,
                &device_cert,
            )
            .await
            .map_err(registry_status)?;

        Ok(Response::new(proto::LinkDeviceResponse {
            user_id: user_id.to_string(),
        }))
    }

    async fn revoke_device(
        &self,
        request: Request<proto::RevokeDeviceRequest>,
    ) -> Result<Response<proto::RevokeDeviceResponse>, Status> {
        let session = resolve_session(&request, &self.registry).await?;
        let user_id = parse_uuid(&request.get_ref().user_id, "user_id")?;
        let device_id = parse_uuid(&request.get_ref().device_id, "device_id")?;
        let target = parse_uuid(&request.get_ref().target_device_id, "target_device_id")?;
        ensure_claims(&session, user_id, device_id)?;

        self.registry
            .revoke_device(session.user_id, target)
            .await
            .map_err(registry_status)?;

        if let Err(err) = self.mailbox.forget_device(session.user_id, target).await {
            tracing::error!(
                error = %err,
                user_id = %session.user_id,
                device_id = %target,
                "device revoked but its mailbox state could not be cleared"
            );
        }

        Ok(Response::new(proto::RevokeDeviceResponse {}))
    }

    async fn lookup_devices(
        &self,
        request: Request<proto::LookupDevicesRequest>,
    ) -> Result<Response<proto::LookupDevicesResponse>, Status> {
        let session = resolve_session(&request, &self.registry).await?;
        let user_id = parse_uuid(&request.get_ref().user_id, "user_id")?;
        let device_id = parse_uuid(&request.get_ref().device_id, "device_id")?;
        let target = parse_uuid(&request.get_ref().target_user_id, "target_user_id")?;
        ensure_claims(&session, user_id, device_id)?;

        let directory = self
            .registry
            .lookup_devices(target)
            .await
            .map_err(registry_status)?;

        Ok(Response::new(proto::LookupDevicesResponse {
            devices: directory.devices.into_iter().map(device_to_proto).collect(),
            user_signing_key: directory.user_signing_key.to_vec(),
        }))
    }
}
