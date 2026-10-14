use std::sync::Arc;

use tokio::sync::{broadcast, watch::Receiver};

use crate::domain::state::facade_events::{FacadeEvent, FacadeEventBus};

pub mod errors;

pub struct FcmClient {}

impl FcmClient {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn send_push(
        &self,
        _token: &str,
        _sender_name: &str,
        _preview_text: &str,
        _data: Option<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
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
