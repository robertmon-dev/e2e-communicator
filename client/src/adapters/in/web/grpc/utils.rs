use tonic::{Code, Request, Status};

use crate::domain::dto::errors::TransportError;
use crate::domain::state::session::SessionState;

const AUTHORIZATION_HEADER: &str = "authorization";

fn classify(status: Status, fallback: fn(String) -> TransportError) -> TransportError {
    let message = status.message().to_string();
    match status.code() {
        Code::Unauthenticated => TransportError::Unauthenticated(message),
        Code::PermissionDenied => TransportError::PermissionDenied(message),
        Code::AlreadyExists => TransportError::AlreadyExists(message),
        Code::NotFound => TransportError::NotFound(message),
        _ => fallback(message),
    }
}

pub fn transport_error(status: Status) -> TransportError {
    classify(status, TransportError::SendFailed)
}

pub fn receive_error(status: Status) -> TransportError {
    classify(status, TransportError::ReceiveFailed)
}

pub fn authorized<T>(session: &SessionState, message: T) -> Request<T> {
    let mut request = Request::new(message);
    if let Some(token) = session.get()
        && let Ok(value) = format!("Bearer {}", token.token).parse()
    {
        request.metadata_mut().insert(AUTHORIZATION_HEADER, value);
    }
    request
}
