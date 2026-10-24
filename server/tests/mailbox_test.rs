mod fixtures;

use common::envelope::{EncryptedEnvelope, EventType};
use server::domain::services::mailbox::{PENDING_CHANNEL, PendingNotification};
use uuid::Uuid;

#[tokio::test]
async fn register_and_get_push_token_round_trips() {
    let mailbox = fixtures::mailbox();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    mailbox
        .register_push_token(user_id, device_id, "test-token-123")
        .await
        .unwrap();

    let token = mailbox.get_push_token(user_id, device_id).await.unwrap();
    assert_eq!(token, Some("test-token-123".to_string()));
}

#[tokio::test]
async fn get_push_token_returns_none_when_unregistered() {
    let mailbox = fixtures::mailbox();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let token = mailbox.get_push_token(user_id, device_id).await.unwrap();
    assert_eq!(token, None);
}

#[tokio::test]
async fn delete_push_token_removes_it() {
    let mailbox = fixtures::mailbox();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    mailbox
        .register_push_token(user_id, device_id, "test-token-123")
        .await
        .unwrap();
    mailbox.delete_push_token(user_id, device_id).await.unwrap();

    let token = mailbox.get_push_token(user_id, device_id).await.unwrap();
    assert_eq!(token, None);
}

#[tokio::test]
async fn is_online_false_before_heartbeat() {
    let mailbox = fixtures::mailbox();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let online = mailbox.is_online(user_id, device_id).await.unwrap();
    assert!(!online);
}

#[tokio::test]
async fn is_online_true_after_heartbeat() {
    let mailbox = fixtures::mailbox();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    mailbox.heartbeat(user_id, device_id).await.unwrap();

    let online = mailbox.is_online(user_id, device_id).await.unwrap();
    assert!(online);
}

#[test]
fn pending_notification_round_trips_through_encode_decode() {
    let notification = PendingNotification {
        user_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        chat_id: Uuid::new_v4(),
        envelope_id: Uuid::new_v4(),
        event_type: EventType::Message,
    };

    let encoded = notification.encode();
    let decoded = PendingNotification::decode(&encoded).unwrap();

    assert_eq!(decoded, notification);
}

#[test]
fn pending_notification_decode_rejects_malformed_payload() {
    let result = PendingNotification::decode("not-enough-fields");
    assert!(result.is_err());
}

#[tokio::test]
async fn fan_out_publishes_pending_notification_for_message_events() {
    use tokio_stream::StreamExt;

    let mailbox = fixtures::mailbox();
    let recipient_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    mailbox
        .register_device(recipient_id, device_id)
        .await
        .unwrap();

    let mut pubsub = mailbox.pubsub_connection().await.unwrap();
    pubsub.subscribe(PENDING_CHANNEL).await.unwrap();
    let mut stream = pubsub.on_message();

    let envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id: Uuid::new_v4(),
        sender_id: Uuid::new_v4(),
        recipient_id,
        created_at: chrono::Utc::now(),
        event_type: EventType::Message,
        ciphertext: vec![1, 2, 3],
    };

    mailbox.fan_out(&envelope).await.unwrap();

    // The pending channel is global, so a concurrently-running test's own
    // publish can arrive first — loop until we see the notification for
    // *this* test's device, rather than assuming the first message is ours.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let notification = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for pending notification"
        );

        let msg = tokio::time::timeout(remaining, stream.next())
            .await
            .expect("timed out waiting for pending notification")
            .expect("pubsub stream ended unexpectedly");

        let payload: String = msg.get_payload().unwrap();
        let notification = PendingNotification::decode(&payload).unwrap();

        if notification.user_id == recipient_id && notification.device_id == device_id {
            break notification;
        }
    };

    assert_eq!(notification.chat_id, envelope.chat_id);
    assert_eq!(notification.envelope_id, envelope.id);
}

#[tokio::test]
async fn fan_out_does_not_publish_for_receipt_events() {
    use tokio_stream::StreamExt;

    let mailbox = fixtures::mailbox();
    let recipient_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    mailbox
        .register_device(recipient_id, device_id)
        .await
        .unwrap();

    let mut pubsub = mailbox.pubsub_connection().await.unwrap();
    pubsub.subscribe(PENDING_CHANNEL).await.unwrap();
    let mut stream = pubsub.on_message();

    let envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id: Uuid::new_v4(),
        sender_id: Uuid::new_v4(),
        recipient_id,
        created_at: chrono::Utc::now(),
        event_type: EventType::DeliveryReceipt,
        ciphertext: vec![1, 2, 3],
    };

    mailbox.fan_out(&envelope).await.unwrap();

    // Global channel, so other concurrently-running tests legitimately
    // publish on it too — only fail if a notification for *this* test's
    // own device shows up.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        let Ok(Some(msg)) = tokio::time::timeout(remaining, stream.next()).await else {
            break;
        };

        let payload: String = msg.get_payload().unwrap();
        if let Ok(notification) = PendingNotification::decode(&payload)
            && notification.user_id == recipient_id
            && notification.device_id == device_id
        {
            panic!("expected no pending notification for a receipt event, got one for this device");
        }
    }
}
