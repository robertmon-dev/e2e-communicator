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
        envelope_id: Uuid::new_v4(),
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
        sender_device_id: Uuid::new_v4(),
        recipient_id,
        recipient_device_id: None,
        created_at: chrono::Utc::now(),
        event_type: EventType::Message,
        notify: true,
        ciphertext: vec![1, 2, 3],
    };

    mailbox.fan_out(&envelope).await.unwrap();

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
        sender_device_id: Uuid::new_v4(),
        recipient_id,
        recipient_device_id: None,
        created_at: chrono::Utc::now(),
        event_type: EventType::DeliveryReceipt,
        notify: false,
        ciphertext: vec![1, 2, 3],
    };

    mailbox.fan_out(&envelope).await.unwrap();

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

#[tokio::test]
async fn fan_out_targets_a_single_device_when_addressed() {
    let mailbox = fixtures::mailbox();
    let recipient = Uuid::new_v4();
    let device_one = Uuid::new_v4();
    let device_two = Uuid::new_v4();
    fixtures::register_devices(&mailbox, recipient, &[device_one, device_two]).await;

    let mut envelope = fixtures::test_envelope(recipient);
    envelope.recipient_device_id = Some(device_one);

    mailbox.fan_out(&envelope).await.unwrap();

    assert_eq!(
        fixtures::stream_len(&mailbox, recipient, device_one).await,
        1
    );
    assert_eq!(
        fixtures::stream_len(&mailbox, recipient, device_two).await,
        0,
        "koperta zaadresowana do jednego urzadzenia nie moze trafic do pozostalych"
    );
}

#[tokio::test]
async fn fan_out_still_broadcasts_when_no_device_is_addressed() {
    let mailbox = fixtures::mailbox();
    let recipient = Uuid::new_v4();
    let device_one = Uuid::new_v4();
    let device_two = Uuid::new_v4();
    fixtures::register_devices(&mailbox, recipient, &[device_one, device_two]).await;

    let mut envelope = fixtures::test_envelope(recipient);
    envelope.recipient_device_id = None;

    mailbox.fan_out(&envelope).await.unwrap();

    assert_eq!(
        fixtures::stream_len(&mailbox, recipient, device_one).await,
        1
    );
    assert_eq!(
        fixtures::stream_len(&mailbox, recipient, device_two).await,
        1
    );
}

#[tokio::test]
async fn pending_notification_carries_no_conversation_metadata() {
    let notification = PendingNotification {
        user_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        envelope_id: Uuid::new_v4(),
    };

    let encoded = notification.encode();

    assert_eq!(encoded.split(':').count(), 3);
    assert_eq!(PendingNotification::decode(&encoded).unwrap(), notification);
}

#[tokio::test]
async fn notify_flag_drives_the_push_decision_not_the_event_type() {
    let mailbox = fixtures::mailbox();
    let recipient = Uuid::new_v4();
    let device = Uuid::new_v4();
    fixtures::register_devices(&mailbox, recipient, &[device]).await;

    // event_type mowiacy "wiadomosc", ale notify=false -> brak powiadomienia
    let mut quiet = fixtures::test_envelope(recipient);
    quiet.recipient_device_id = Some(device);
    quiet.event_type = EventType::Message;
    quiet.notify = false;

    mailbox.fan_out(&quiet).await.unwrap();

    // koperta wciaz trafia do skrzynki, tylko bez budzenia
    assert_eq!(fixtures::stream_len(&mailbox, recipient, device).await, 1);
}

#[tokio::test]
async fn forget_device_clears_every_trace_of_the_device() {
    let mailbox = fixtures::mailbox();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    mailbox.register_device(user_id, device_id).await.unwrap();
    mailbox
        .register_push_token(user_id, device_id, "fcm-token")
        .await
        .unwrap();
    mailbox.heartbeat(user_id, device_id).await.unwrap();

    mailbox.forget_device(user_id, device_id).await.unwrap();

    assert!(!mailbox.is_online(user_id, device_id).await.unwrap());
    assert_eq!(
        mailbox.get_push_token(user_id, device_id).await.unwrap(),
        None
    );

    let mut conn = fixtures::store().connection().await.unwrap();
    let members: Vec<String> =
        redis::AsyncCommands::smembers(&mut conn, format!("devices:{user_id}"))
            .await
            .unwrap();
    assert!(!members.contains(&device_id.to_string()));

    let stream_exists: bool =
        redis::AsyncCommands::exists(&mut conn, format!("mailbox:{user_id}:{device_id}"))
            .await
            .unwrap();
    assert!(!stream_exists);
}

#[tokio::test]
async fn fan_out_drops_envelopes_addressed_to_unregistered_devices() {
    let mailbox = fixtures::mailbox();
    let recipient = Uuid::new_v4();
    let known_device = Uuid::new_v4();
    let stranger_device = Uuid::new_v4();
    fixtures::register_devices(&mailbox, recipient, &[known_device]).await;

    let mut envelope = fixtures::test_envelope(recipient);
    envelope.recipient_device_id = Some(stranger_device);

    mailbox.fan_out(&envelope).await.unwrap();

    assert_eq!(
        fixtures::stream_len(&mailbox, recipient, stranger_device).await,
        0,
        "nadawca nie moze zakladac strumieni dla nieistniejacych urzadzen"
    );
    assert_eq!(
        fixtures::stream_len(&mailbox, recipient, known_device).await,
        0
    );
}
