mod fixtures;

use std::sync::Mutex;

use server::adapters::out::push::PushSender;
use server::adapters::out::push::errors::FcmError;
use server::domain::services::mailbox::PendingNotification;
use server::domain::services::push::dispatch;
use uuid::Uuid;

fn test_notification(user_id: Uuid, device_id: Uuid) -> PendingNotification {
    PendingNotification {
        user_id,
        device_id,
        envelope_id: Uuid::new_v4(),
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

impl PushSender for FakePushSender {
    async fn send_push(&self, token: &str, data: serde_json::Value) -> Result<(), FcmError> {
        self.calls.lock().unwrap().push((token.to_string(), data));
        (self.result)()
    }
}

async fn dispatched_push_payload(notification: PendingNotification) -> Option<serde_json::Value> {
    let mailbox = fixtures::mailbox();
    mailbox
        .register_push_token(notification.user_id, notification.device_id, "tok")
        .await
        .unwrap();

    let sender = FakePushSender::new(|| Ok(()));
    dispatch(&mailbox, &sender, notification).await;

    sender
        .calls
        .lock()
        .unwrap()
        .pop()
        .map(|(_, payload)| payload)
}

#[tokio::test]
async fn push_payload_carries_no_conversation_metadata() {
    let payload = dispatched_push_payload(PendingNotification {
        user_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        envelope_id: Uuid::new_v4(),
    })
    .await
    .expect("push wyslany");

    assert!(payload.get("chat_id").is_none());
    assert!(payload.get("event_type").is_none());
    assert!(payload.get("envelope_id").is_some());
}

#[tokio::test]
async fn dispatch_skips_online_device() {
    let mailbox = fixtures::mailbox();
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
    let mailbox = fixtures::mailbox();
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
    let mailbox = fixtures::mailbox();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let sender = FakePushSender::new(|| Ok(()));
    dispatch(&mailbox, &sender, test_notification(user_id, device_id)).await;

    assert!(sender.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn dispatch_deletes_token_on_invalid_token_error() {
    let mailbox = fixtures::mailbox();
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
