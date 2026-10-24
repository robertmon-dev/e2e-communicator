use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::adapters::r#in::grpc::proto::identity as proto;
use crate::adapters::r#in::grpc::proto::identity::auth_server::Auth;
use crate::adapters::r#in::grpc::utils::{parse_signature, parse_uuid, registry_status};
use crate::domain::services::registry::Registry;

pub struct AuthService {
    registry: Arc<Registry>,
}

impl AuthService {
    pub fn new(registry: Arc<Registry>) -> Self {
        Self { registry }
    }
}

#[tonic::async_trait]
impl Auth for AuthService {
    async fn challenge(
        &self,
        request: Request<proto::ChallengeRequest>,
    ) -> Result<Response<proto::ChallengeResponse>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid(&req.user_id, "user_id")?;
        let device_id = parse_uuid(&req.device_id, "device_id")?;

        let nonce = self
            .registry
            .issue_challenge(user_id, device_id)
            .await
            .map_err(registry_status)?;

        Ok(Response::new(proto::ChallengeResponse {
            nonce: nonce.to_vec(),
        }))
    }

    async fn authenticate(
        &self,
        request: Request<proto::AuthenticateRequest>,
    ) -> Result<Response<proto::AuthenticateResponse>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid(&req.user_id, "user_id")?;
        let device_id = parse_uuid(&req.device_id, "device_id")?;
        let signature = parse_signature(&req.signature, "signature")?;

        let session = self
            .registry
            .authenticate(user_id, device_id, &signature)
            .await
            .map_err(registry_status)?;

        Ok(Response::new(proto::AuthenticateResponse {
            token: session.token,
            expires_at_unix_ms: session.expires_at.timestamp_millis(),
        }))
    }
}
