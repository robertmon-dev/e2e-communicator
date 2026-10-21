use std::path::Path;

use serde_json::Value;
use yup_oauth2::{ServiceAccountAuthenticator, authenticator::DefaultAuthenticator};

use crate::adapters::out::push::consts::{FCM_SCOPE, HTTP_TIMEOUT};
use crate::adapters::out::push::errors::FcmError;
use crate::adapters::out::push::utils::{is_unregistered, stringify_data};

pub mod consts;
pub mod errors;
pub mod utils;

pub trait PushSender {
    async fn send_push(&self, token: &str, data: Value) -> Result<(), FcmError>;
}

pub struct FcmClient {
    http: reqwest::Client,
    auth: DefaultAuthenticator,
    send_url: String,
}

impl FcmClient {
    pub async fn from_service_account_file(path: impl AsRef<Path>) -> Result<Self, FcmError> {
        let key = yup_oauth2::read_service_account_key(path)
            .await
            .map_err(FcmError::Init)?;
        Self::from_service_account_key(key).await
    }

    pub async fn from_service_account_json(json: &str) -> Result<Self, FcmError> {
        let key = yup_oauth2::parse_service_account_key(json).map_err(FcmError::Init)?;
        Self::from_service_account_key(key).await
    }

    async fn from_service_account_key(
        key: yup_oauth2::ServiceAccountKey,
    ) -> Result<Self, FcmError> {
        let project_id = key.project_id.clone().ok_or(FcmError::MissingProjectId)?;

        let auth = ServiceAccountAuthenticator::builder(key)
            .build()
            .await
            .map_err(FcmError::Init)?;

        auth.token(&[FCM_SCOPE]).await?;

        let http = reqwest::Client::builder().timeout(HTTP_TIMEOUT).build()?;

        tracing::info!(%project_id, "FCM client initialized");

        Ok(Self {
            http,
            auth,
            send_url: format!("https://fcm.googleapis.com/v1/projects/{project_id}/messages:send"),
        })
    }

    async fn access_token(&self) -> Result<String, FcmError> {
        let token = self.auth.token(&[FCM_SCOPE]).await?;
        token.token().map(str::to_owned).ok_or(FcmError::EmptyToken)
    }
}

impl PushSender for FcmClient {
    async fn send_push(&self, token: &str, data: Value) -> Result<(), FcmError> {
        let data_map = stringify_data(data).unwrap_or_default();

        let message = serde_json::json!({
            "token": token,
            "data": data_map,
            "android": { "priority": "high" },
            "apns": { "headers": { "apns-priority": "10" } },
        });

        let response = self
            .http
            .post(&self.send_url)
            .bearer_auth(self.access_token().await?)
            .json(&serde_json::json!({ "message": message }))
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        let body = response.text().await.unwrap_or_default();
        if is_unregistered(status, &body) {
            return Err(FcmError::InvalidToken);
        }
        Err(FcmError::Api { status, body })
    }
}
