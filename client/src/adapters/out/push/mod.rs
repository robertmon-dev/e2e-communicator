use std::{path::Path, sync::Arc};

use serde_json::Value;
use tokio::sync::{broadcast, watch::Receiver};
use yup_oauth2::{ServiceAccountAuthenticator, authenticator::DefaultAuthenticator};

use crate::{
    adapters::out::push::consts::{FCM_SCOPE, HTTP_TIMEOUT},
    adapters::out::push::errors::FcmError,
    adapters::out::push::utils::{is_unregistered, stringify_data},
    domain::state::facade_events::{FacadeEvent, FacadeEventBus},
};

pub mod consts;
pub mod errors;
pub mod utils;

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

    pub async fn send_push(
        &self,
        token: &str,
        sender_name: &str,
        preview_text: &str,
        data: Option<Value>,
    ) -> Result<(), FcmError> {
        let mut message = serde_json::json!({
            "token": token,
            "notification": {
                "title": sender_name,
                "body": preview_text,
            },
            "android": { "priority": "high" },
            "apns": { "headers": { "apns-priority": "10" } },
        });

        if let Some(data) = data.and_then(stringify_data) {
            message["data"] = Value::Object(data);
        }

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

pub async fn run(fcm_client: Arc<FcmClient>, events: FacadeEventBus, mut shutdown: Receiver<bool>) {
    let mut rx = events.subscribe();
    tracing::info!("FCM dispatch daemon intialized!");

    if *shutdown.borrow_and_update() {
        return;
    }

    loop {
        tokio::select! {
            biased;

            res = shutdown.wait_for(|&down| down) => {
                if res.is_err() {
                    tracing::warn!("Shutdown sender dropped, terminating FCM daemon");
                } else {
                    tracing::info!("FCM daemon received shutdown signal, exiting.");
                }
                break;
            }

            event = rx.recv() => {
                match event {
                    Ok(FacadeEvent::MessageReceived(message)) => {
                        let client = Arc::clone(&fcm_client);

                        let message_id = message.id;
                        let chat_id = message.chat_id;
                        let preview_text = message.get_preview_text().to_string();
                        let sender_name = message.author_id.to_string();

                        tokio::spawn(async move {
                            let recipient_tokens: Vec<String> = Vec::new();

                            let data = serde_json::json!({
                                "chat_id": chat_id.to_string(),
                                "message_id": message_id.to_string(),
                            });

                            let mut tasks = tokio::task::JoinSet::new();

                            for token in recipient_tokens {
                                let client = Arc::clone(&client);
                                let sender_name = sender_name.clone();
                                let preview_text = preview_text.clone();
                                let data = data.clone();

                                tasks.spawn(async move {
                                    if let Err(e) = client
                                        .send_push(&token, &sender_name, &preview_text, Some(data))
                                        .await
                                    {
                                        tracing::warn!(target_token = %token, error = %e, "Failed to dispatch push");
                                    }
                                });
                            }

                            while tasks.join_next().await.is_some() {}
                        });
                    }
                    Ok(_) => {},
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("FCM event subscriber lagged, dropped {skipped} notifications");
                    },
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::warn!("Facade event bus closed, terminating FCM daemon");
                        break;
                    }
                }
            }

        }
    }
}
