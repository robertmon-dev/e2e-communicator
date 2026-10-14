use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum FcmError {
    #[error("FCM init failed: {0}")]
    Init(#[source] std::io::Error),
    #[error("service account key has no project_id")]
    MissingProjectId,
    #[error("OAuth2 error: {0}")]
    Auth(#[from] yup_oauth2::Error),
    #[error("OAuth2 response contained no access token")]
    EmptyToken,
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("registration token is no longer valid")]
    InvalidToken,
    #[error("FCM returned {status}: {body}")]
    Api { status: StatusCode, body: String },
}
