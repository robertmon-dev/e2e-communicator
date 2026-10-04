mod fixtures;

use std::sync::Arc;

use chrono::Utc;
use orion::aead::SecretKey;
use uuid::Uuid;

use client::adapters::out::persistance::repositories::key::KeyRepository;
use client::adapters::out::persistance::repositories::message::MessageRepository;
use client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use client::adapters::out::persistance::repositories::outbox::create::Create as OutboxCreate;
use client::domain::dto::delivery_receipt::DeliveryReceipt;
use client::domain::dto::message::{MessageContent, MessageWirePayload};
use client::domain::dto::outbox::{OutboxEntry, OutboxStatus};
use client::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};
use client::domain::services::exchanger::Exchanger;
use client::domain::services::inbox::Inbox;
use client::domain::state::connectivity::ConnectivityState;
use client::domain::state::facade_events::{FacadeEvent, facade_event_bus};
use common::envelope::{EncryptedEnvelope, EventType};
use fixtures::{NoopSubscription, seed_chat, seed_message, seed_user, setup_db};

#[tokio::test]
async fn handle_message_decrypts_and_persists_it() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let local_user_id = Uuid::new_v4();
    let sender_id = Uuid::new_v4();

    seed_user(&db, local_user_id).await;
    seed_user(&db, sender_id).await;
    seed_chat(&db, chat_id).await;

    let parent_message_id = Uuid::new_v4();
    seed_message(&db, parent_message_id, sender_id, chat_id).await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository, SecretKey::default()));
    exchanger
        .generate_local_key(chat_id, local_user_id)
        .await
        .unwrap();
    exchanger
        .generate_local_key(chat_id, sender_id)
        .await
        .unwrap();

    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let events = facade_event_bus();
    let mut event_receiver = events.subscribe();

    let inbox = Inbox::new(
        Arc::new(NoopSubscription),
        message_repository.clone(),
        outbox_repository.clone(),
        exchanger.clone(),
        Arc::new(ConnectivityState::default()),
        local_user_id,
        Uuid::new_v4(),
        events,
    );

    let message_id = Uuid::new_v4();
    let reply_to_id = Some(parent_message_id);
    let payload = MessageWirePayload {
        message_id,
        reply_to_id,
        available_to: None,
        content: MessageContent {
            id: Uuid::new_v4(),
            text: "hey there".to_string(),
            attachments: Vec::new(),
            reactions: Vec::new(),
        },
    };
    let plaintext = serde_json::to_vec(&payload).unwrap();

    let shared_key = exchanger
        .derive_shared_key(chat_id, sender_id, local_user_id)
        .await
        .unwrap();
    let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext).unwrap();

    let envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id,
        recipient_id: local_user_id,
        created_at: Utc::now(),
        event_type: EventType::Message,
        ciphertext,
    };

    inbox.handle(Uuid::new_v4().to_string(), envelope).await;

    let stored = message_repository.get(message_id).await.unwrap();
    assert_eq!(stored.id, message_id);
    assert_eq!(stored.author_id, sender_id);
    assert_eq!(stored.reply_to_id, reply_to_id);
    assert!(stored.is_reply);
    assert_eq!(stored.get_latest_with_date().unwrap().1.text, "hey there");

    let receipt_entry = outbox_repository
        .get_by_message_and_recipient(message_id, sender_id)
        .await
        .unwrap();
    assert_eq!(receipt_entry.recipient_id, sender_id);
    assert_eq!(receipt_entry.event_type, EventType::DeliveryReceipt);

    let event = event_receiver.try_recv().unwrap();
    match event {
        FacadeEvent::MessageReceived(received) => assert_eq!(received.id, message_id),
        other => panic!("expected MessageReceived, got {other:?}"),
    }
}

#[tokio::test]
async fn handle_delivery_receipt_marks_outbox_entry_delivered() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let local_user_id = Uuid::new_v4();
    let peer_id = Uuid::new_v4();

    seed_user(&db, local_user_id).await;
    seed_user(&db, peer_id).await;
    seed_chat(&db, chat_id).await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository, SecretKey::default()));
    exchanger
        .generate_local_key(chat_id, local_user_id)
        .await
        .unwrap();
    exchanger
        .generate_local_key(chat_id, peer_id)
        .await
        .unwrap();

    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let inbox = Inbox::new(
        Arc::new(NoopSubscription),
        message_repository,
        outbox_repository.clone(),
        exchanger.clone(),
        Arc::new(ConnectivityState::default()),
        local_user_id,
        Uuid::new_v4(),
    );

    let message_id = Uuid::new_v4();
    seed_message(&db, message_id, local_user_id, chat_id).await;

    let original_envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id: local_user_id,
        recipient_id: peer_id,
        created_at: Utc::now(),
        event_type: EventType::Message,
        ciphertext: Vec::new(),
    };
    let outbox_entry = OutboxEntry::pending(message_id, peer_id, &original_envelope, None);
    outbox_repository
        .create_many(vec![outbox_entry])
        .await
        .unwrap();

    let receipt = DeliveryReceipt {
        id: Uuid::new_v4(),
        message_id,
        chat_id,
        user_id: peer_id,
    };
    let plaintext = serde_json::to_vec(&receipt).unwrap();

    let shared_key = exchanger
        .derive_shared_key(chat_id, peer_id, local_user_id)
        .await
        .unwrap();
    let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext).unwrap();

    let envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id: peer_id,
        recipient_id: local_user_id,
        created_at: Utc::now(),
        event_type: EventType::DeliveryReceipt,
        ciphertext,
    };

    inbox.handle(Uuid::new_v4().to_string(), envelope).await;

    let updated = outbox_repository
        .get_by_message_and_recipient(message_id, peer_id)
        .await
        .unwrap();
    assert_eq!(updated.status, OutboxStatus::Delivered);
}
