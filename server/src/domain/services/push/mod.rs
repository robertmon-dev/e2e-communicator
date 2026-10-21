use std::sync::Arc;
use std::time::Duration;

use tokio_stream::StreamExt;

use crate::adapters::out::push::PushSender;
use crate::adapters::out::push::errors::FcmError;
use crate::domain::services::mailbox::{Mailbox, PendingNotification};

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

pub async fn run<T: PushSender>(mailbox: Arc<Mailbox>, push_sender: Arc<T>) {
    loop {
        let mut pubsub = match mailbox.pubsub_connection().await {
            Ok(pubsub) => pubsub,
            Err(err) => {
                tracing::error!(error = %err, "failed to open push pubsub connection, retrying");
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            }
        };

        if let Err(err) = pubsub
            .subscribe(crate::domain::services::mailbox::PENDING_CHANNEL)
            .await
        {
            tracing::error!(error = %err, "failed to subscribe to mailbox:pending, retrying");
            tokio::time::sleep(RECONNECT_DELAY).await;
            continue;
        }

        tracing::info!("push dispatch daemon subscribed to mailbox:pending");

        let mut stream = pubsub.into_on_message();

        while let Some(msg) = stream.next().await {
            let payload: String = match msg.get_payload() {
                Ok(payload) => payload,
                Err(err) => {
                    tracing::warn!(error = %err, "failed to read mailbox:pending payload");
                    continue;
                }
            };

            let notification = match PendingNotification::decode(&payload) {
                Ok(notification) => notification,
                Err(err) => {
                    tracing::warn!(error = %err, %payload, "failed to decode mailbox:pending payload");
                    continue;
                }
            };

            dispatch(&mailbox, &*push_sender, notification).await;
        }

        tracing::warn!("push pubsub stream ended, reconnecting");
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn dispatch<T: PushSender>(
    mailbox: &Mailbox,
    push_sender: &T,
    notification: PendingNotification,
) {
    let PendingNotification {
        user_id,
        device_id,
        chat_id,
        envelope_id,
        event_type,
    } = notification;

    match mailbox.is_online(user_id, device_id).await {
        Ok(true) => return,
        Ok(false) => {}
        Err(err) => {
            tracing::warn!(error = %err, %user_id, %device_id, "failed to check device liveness");
            return;
        }
    }

    let token = match mailbox.get_push_token(user_id, device_id).await {
        Ok(Some(token)) => token,
        Ok(None) => return,
        Err(err) => {
            tracing::warn!(error = %err, %user_id, %device_id, "failed to look up push token");
            return;
        }
    };

    let data = serde_json::json!({
        "chat_id": chat_id.to_string(),
        "envelope_id": envelope_id.to_string(),
        "event_type": (event_type as i32).to_string(),
    });

    if let Err(err) = push_sender.send_push(&token, data).await {
        tracing::warn!(error = %err, %user_id, %device_id, "failed to dispatch push");

        if matches!(err, FcmError::InvalidToken)
            && let Err(del_err) = mailbox.delete_push_token(user_id, device_id).await
        {
            tracing::warn!(error = %del_err, %user_id, %device_id, "failed to delete invalid push token");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use common::envelope::EventType;
    use uuid::Uuid;

    use crate::adapters::out::push::errors::FcmError;
    use crate::core::initializers::settings::RedisSettings;
    use crate::domain::services::mailbox::{Mailbox, PendingNotification};

    use super::*;

    async fn test_mailbox() -> Mailbox {
        let settings = RedisSettings {
            sentinel_nodes: vec![
                "localhost:26379".to_string(),
                "localhost:26380".to_string(),
                "localhost:26381".to_string(),
            ],
            service_name: "mymaster".to_string(),
            password: None,
        };
        let store = crate::core::initializers::redis::MailboxStore::new(&settings)
            .expect("failed to build MailboxStore - is `make redis-up` running?");
        Mailbox::new(store)
    }

    fn test_notification(user_id: Uuid, device_id: Uuid) -> PendingNotification {
        PendingNotification {
            user_id,
            device_id,
            chat_id: Uuid::new_v4(),
            envelope_id: Uuid::new_v4(),
            event_type: EventType::Message,
        }
    }

    struct FakePushSender {
        calls: Mutex<Vec<(String, serde_json::Value)>>,
        result: fn() -> Result<(), FcmError>,
    }

    impl FakePushSender {
        fn new(result: fn() -> Result<(), FcmError>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                result,
            }
        }
    }

    impl crate::adapters::out::push::PushSender for FakePushSender {
        async fn send_push(&self, token: &str, data: serde_json::Value) -> Result<(), FcmError> {
            self.calls.lock().unwrap().push((token.to_string(), data));
            (self.result)()
        }
    }

    #[tokio::test]
    async fn dispatch_skips_online_device() {
        let mailbox = test_mailbox().await;
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        mailbox.heartbeat(user_id, device_id).await.unwrap();
        mailbox
            .register_push_token(user_id, device_id, "tok")
            .await
            .unwrap();

        let sender = FakePushSender::new(|| Ok(()));
        dispatch(&mailbox, &sender, test_notification(user_id, device_id)).await;

        assert!(sender.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatch_sends_for_offline_device_with_token() {
        let mailbox = test_mailbox().await;
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        mailbox
            .register_push_token(user_id, device_id, "tok")
            .await
            .unwrap();

        let sender = FakePushSender::new(|| Ok(()));
        dispatch(&mailbox, &sender, test_notification(user_id, device_id)).await;

        let calls = sender.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "tok");
    }

    #[tokio::test]
    async fn dispatch_skips_offline_device_without_token() {
        let mailbox = test_mailbox().await;
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        let sender = FakePushSender::new(|| Ok(()));
        dispatch(&mailbox, &sender, test_notification(user_id, device_id)).await;

        assert!(sender.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatch_deletes_token_on_invalid_token_error() {
        let mailbox = test_mailbox().await;
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        mailbox
            .register_push_token(user_id, device_id, "tok")
            .await
            .unwrap();

        let sender = FakePushSender::new(|| Err(FcmError::InvalidToken));
        dispatch(&mailbox, &sender, test_notification(user_id, device_id)).await;

        let token = mailbox.get_push_token(user_id, device_id).await.unwrap();
        assert_eq!(token, None);
    }
}
