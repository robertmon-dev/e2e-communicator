use chrono::{Duration, Utc};
use client::adapters::r#in::web::grpc::utils::{authorized, receive_error, transport_error};
use client::domain::dto::errors::TransportError;
use client::domain::state::session::{SessionState, SessionToken};
use tonic::{Code, Status};

#[test]
fn status_codes_map_to_transport_errors() {
    assert!(matches!(
        transport_error(Status::new(Code::Unauthenticated, "x")),
        TransportError::Unauthenticated(_)
    ));
    assert!(matches!(
        transport_error(Status::new(Code::PermissionDenied, "x")),
        TransportError::PermissionDenied(_)
    ));
    assert!(matches!(
        transport_error(Status::new(Code::AlreadyExists, "x")),
        TransportError::AlreadyExists(_)
    ));
    assert!(matches!(
        transport_error(Status::new(Code::NotFound, "x")),
        TransportError::NotFound(_)
    ));
    assert!(matches!(
        transport_error(Status::new(Code::Unavailable, "x")),
        TransportError::SendFailed(_)
    ));
    assert!(matches!(
        receive_error(Status::new(Code::Unavailable, "x")),
        TransportError::ReceiveFailed(_)
    ));
    assert!(matches!(
        receive_error(Status::new(Code::Unauthenticated, "x")),
        TransportError::Unauthenticated(_)
    ));
}

#[test]
fn authorized_attaches_a_bearer_header_only_when_a_session_exists() {
    let session = SessionState::default();

    let without = authorized(&session, ());
    assert!(without.metadata().get("authorization").is_none());

    session.set(SessionToken {
        token: "tok123".to_string(),
        expires_at: Utc::now() + Duration::hours(1),
    });
    let with = authorized(&session, ());
    assert_eq!(
        with.metadata()
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        "Bearer tok123"
    );
}
