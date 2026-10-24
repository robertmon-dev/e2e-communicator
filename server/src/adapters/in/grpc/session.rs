use tonic::{Request, Status};
use uuid::Uuid;

use crate::adapters::r#in::grpc::utils::registry_status;
use crate::domain::services::registry::{Registry, Session};

const AUTHORIZATION_HEADER: &str = "authorization";
const BEARER_PREFIX: &str = "Bearer ";

fn bearer_token<T>(request: &Request<T>) -> Result<&str, Status> {
    let header = request
        .metadata()
        .get(AUTHORIZATION_HEADER)
        .ok_or_else(|| Status::unauthenticated("missing authorization header"))?;
    let value = header
        .to_str()
        .map_err(|_| Status::unauthenticated("invalid authorization header"))?;
    value
        .strip_prefix(BEARER_PREFIX)
        .ok_or_else(|| Status::unauthenticated("invalid authorization header"))
}

pub async fn resolve_session<T>(
    request: &Request<T>,
    registry: &Registry,
) -> Result<Session, Status> {
    let token = bearer_token(request)?;

    registry
        .session(token)
        .await
        .map_err(registry_status)?
        .ok_or_else(|| Status::unauthenticated("session unknown or expired"))
}

pub fn ensure_claims(
    session: &Session,
    claimed_user: Uuid,
    claimed_device: Uuid,
) -> Result<(), Status> {
    if session.user_id != claimed_user || session.device_id != claimed_device {
        tracing::warn!(
            session_user = %session.user_id,
            session_device = %session.device_id,
            %claimed_user,
            %claimed_device,
            "request claims do not match session"
        );
        return Err(Status::permission_denied("request does not match session"));
    }

    Ok(())
}

pub async fn require_session<T>(
    request: &Request<T>,
    registry: &Registry,
    claimed_user: Uuid,
    claimed_device: Uuid,
) -> Result<Session, Status> {
    let session = resolve_session(request, registry).await?;
    ensure_claims(&session, claimed_user, claimed_device)?;
    Ok(session)
}
