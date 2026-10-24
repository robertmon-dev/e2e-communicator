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

pub async fn dispatch<T: PushSender>(
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
