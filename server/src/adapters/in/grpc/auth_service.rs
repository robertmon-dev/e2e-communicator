use tonic::{Request, Response, Status};

use crate::adapters::r#in::grpc::proto::identity as proto;
use crate::adapters::r#in::grpc::proto::identity::auth_server::Auth;

pub struct AuthService;

#[tonic::async_trait]
impl Auth for AuthService {
    async fn challenge(
        &self,
        _request: Request<proto::ChallengeRequest>,
    ) -> Result<Response<proto::ChallengeResponse>, Status> {
        Err(Status::unimplemented("challenge not yet implemented"))
    }

    async fn authenticate(
        &self,
        _request: Request<proto::AuthenticateRequest>,
    ) -> Result<Response<proto::AuthenticateResponse>, Status> {
        Err(Status::unimplemented("authenticate not yet implemented"))
    }
}
