use tonic::{Request, Response, Status};

use crate::adapters::r#in::grpc::proto::identity as proto;
use crate::adapters::r#in::grpc::proto::identity::register_server::Register;

pub struct RegisterService;

#[tonic::async_trait]
impl Register for RegisterService {
    async fn register_device(
        &self,
        _request: Request<proto::RegisterDeviceRequest>,
    ) -> Result<Response<proto::RegisterDeviceResponse>, Status> {
        Err(Status::unimplemented("register_device not yet implemented"))
    }

    async fn create_link_token(
        &self,
        _request: Request<proto::CreateLinkTokenRequest>,
    ) -> Result<Response<proto::CreateLinkTokenResponse>, Status> {
        Err(Status::unimplemented(
            "create_link_token not yet implemented",
        ))
    }

    async fn link_device(
        &self,
        _request: Request<proto::LinkDeviceRequest>,
    ) -> Result<Response<proto::LinkDeviceResponse>, Status> {
        Err(Status::unimplemented("link_device not yet implemented"))
    }

    async fn revoke_device(
        &self,
        _request: Request<proto::RevokeDeviceRequest>,
    ) -> Result<Response<proto::RevokeDeviceResponse>, Status> {
        Err(Status::unimplemented("revoke_device not yet implemented"))
    }

    async fn lookup_devices(
        &self,
        _request: Request<proto::LookupDevicesRequest>,
    ) -> Result<Response<proto::LookupDevicesResponse>, Status> {
        Err(Status::unimplemented("lookup_devices not yet implemented"))
    }
}
