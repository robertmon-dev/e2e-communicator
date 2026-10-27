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
use client::domain::state::identity::{IdentityState, IdentityStatus};
use client::domain::state::session::{SessionState, SessionToken};
use common::envelope::{EncryptedEnvelope, EventType};
use fixtures::{NoopSubscription, inbox_support, seed_chat, seed_message, seed_user, setup_db};

fn registered(
    local_user_id: Uuid,
    local_device_id: Uuid,
) -> (Arc<IdentityState>, Arc<SessionState>) {
    let identity = client::domain::dto::local_identity::LocalIdentity {
        user_id: local_user_id,
        device_id: local_device_id,
        agreement_public_key: vec![1; 32],
        signing_public_key: vec![2; 32],
        device_cert: vec![3; 64],
        created_at: Utc::now(),
    };

    let session = Arc::new(SessionState::default());
    session.set(SessionToken {
        token: "test-session".to_string(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    });

    (
        Arc::new(IdentityState::new(IdentityStatus::Registered(identity))),
        session,
    )
}

#[tokio::test]
async fn handle_message_decrypts_and_persists_it() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let local_user_id = Uuid::new_v4();
    let local_device_id = Uuid::new_v4();
    let sender_id = Uuid::new_v4();
    let sender_device_id = Uuid::new_v4();

    seed_user(&db, local_user_id).await;
    seed_user(&db, sender_id).await;
    seed_chat(&db, chat_id).await;

    let parent_message_id = Uuid::new_v4();
    seed_message(&db, parent_message_id, sender_id, chat_id).await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository, SecretKey::default()));
    exchanger
        .generate_local_key(chat_id, local_user_id, local_device_id)
        .await
        .unwrap();
    exchanger
        .generate_local_key(chat_id, sender_id, sender_device_id)
        .await
        .unwrap();

    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let events = facade_event_bus();
    let mut event_receiver = events.subscribe();
    let (identity_state, session_state) = registered(local_user_id, local_device_id);
    let (
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
    ) = inbox_support(&db, &identity_state, &exchanger, &outbox_repository);

    let inbox = Inbox::new(
        Arc::new(NoopSubscription),
        message_repository.clone(),
        outbox_repository.clone(),
        exchanger.clone(),
        Arc::new(ConnectivityState::default()),
        identity_state,
        session_state,
        events,
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
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
        .derive_shared_key(
            chat_id,
            (sender_id, sender_device_id),
            (local_user_id, local_device_id),
        )
        .await
        .unwrap();
    let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext).unwrap();

    let envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id,
        sender_device_id,
        recipient_id: local_user_id,
        recipient_device_id: Some(local_device_id),
        created_at: Utc::now(),
        event_type: EventType::Message,
        notify: true,
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
        .get_by_message_recipient_device(message_id, sender_id, sender_device_id)
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
    let local_device_id = Uuid::new_v4();
    let peer_id = Uuid::new_v4();
    let peer_device_id = Uuid::new_v4();

    seed_user(&db, local_user_id).await;
    seed_user(&db, peer_id).await;
    seed_chat(&db, chat_id).await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository, SecretKey::default()));
    exchanger
        .generate_local_key(chat_id, local_user_id, local_device_id)
        .await
        .unwrap();
    exchanger
        .generate_local_key(chat_id, peer_id, peer_device_id)
        .await
        .unwrap();

    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let events = facade_event_bus();
    let mut event_receiver = events.subscribe();
    let (identity_state, session_state) = registered(local_user_id, local_device_id);
    let (
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
    ) = inbox_support(&db, &identity_state, &exchanger, &outbox_repository);

    let inbox = Inbox::new(
        Arc::new(NoopSubscription),
        message_repository,
        outbox_repository.clone(),
        exchanger.clone(),
        Arc::new(ConnectivityState::default()),
        identity_state,
        session_state,
        events,
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
    );

    let message_id = Uuid::new_v4();
    seed_message(&db, message_id, local_user_id, chat_id).await;

    let original_envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id: local_user_id,
        sender_device_id: local_device_id,
        recipient_id: peer_id,
        recipient_device_id: Some(peer_device_id),
        created_at: Utc::now(),
        event_type: EventType::Message,
        notify: true,
        ciphertext: Vec::new(),
    };
    let outbox_entry = OutboxEntry::pending(Some(message_id), peer_id, &original_envelope, None);
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
        .derive_shared_key(
            chat_id,
            (peer_id, peer_device_id),
            (local_user_id, local_device_id),
        )
        .await
        .unwrap();
    let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext).unwrap();

    let envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id: peer_id,
        sender_device_id: peer_device_id,
        recipient_id: local_user_id,
        recipient_device_id: Some(local_device_id),
        created_at: Utc::now(),
        event_type: EventType::DeliveryReceipt,
        notify: false,
        ciphertext,
    };

    inbox.handle(Uuid::new_v4().to_string(), envelope).await;

    let updated = outbox_repository
        .get_by_message_recipient_device(message_id, peer_id, peer_device_id)
        .await
        .unwrap();
    assert_eq!(updated.status, OutboxStatus::Delivered);

    let event = event_receiver.try_recv().unwrap();
    match event {
        FacadeEvent::DeliveryReceiptMarked {
            message_id: received_id,
            recipient_id,
        } => {
            assert_eq!(received_id, message_id);
            assert_eq!(recipient_id, peer_id);
        }
        other => panic!("expected DeliveryReceiptMarked, got {other:?}"),
    }
}

#[tokio::test]
async fn a_delivery_receipt_from_one_device_leaves_the_others_device_entry_pending() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let local_user_id = Uuid::new_v4();
    let local_device_id = Uuid::new_v4();
    let peer_id = Uuid::new_v4();
    let peer_device_a = Uuid::new_v4();
    let peer_device_b = Uuid::new_v4();

    seed_user(&db, local_user_id).await;
    seed_user(&db, peer_id).await;
    seed_chat(&db, chat_id).await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository, SecretKey::default()));
    exchanger
        .generate_local_key(chat_id, local_user_id, local_device_id)
        .await
        .unwrap();
    exchanger
        .generate_local_key(chat_id, peer_id, peer_device_a)
        .await
        .unwrap();

    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let (identity_state, session_state) = registered(local_user_id, local_device_id);
    let (
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
    ) = inbox_support(&db, &identity_state, &exchanger, &outbox_repository);

    let inbox = Inbox::new(
        Arc::new(NoopSubscription),
        message_repository,
        outbox_repository.clone(),
        exchanger.clone(),
        Arc::new(ConnectivityState::default()),
        identity_state,
        session_state,
        facade_event_bus(),
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
    );

    let message_id = Uuid::new_v4();
    seed_message(&db, message_id, local_user_id, chat_id).await;

    let envelope_to_device_a = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id: local_user_id,
        sender_device_id: local_device_id,
        recipient_id: peer_id,
        recipient_device_id: Some(peer_device_a),
        created_at: Utc::now(),
        event_type: EventType::Message,
        notify: true,
        ciphertext: Vec::new(),
    };
    let envelope_to_device_b = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id: local_user_id,
        sender_device_id: local_device_id,
        recipient_id: peer_id,
        recipient_device_id: Some(peer_device_b),
        created_at: Utc::now(),
        event_type: EventType::Message,
        notify: true,
        ciphertext: Vec::new(),
    };

    outbox_repository
        .create_many(vec![
            OutboxEntry::pending(Some(message_id), peer_id, &envelope_to_device_a, None),
            OutboxEntry::pending(Some(message_id), peer_id, &envelope_to_device_b, None),
        ])
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
        .derive_shared_key(
            chat_id,
            (peer_id, peer_device_a),
            (local_user_id, local_device_id),
        )
        .await
        .unwrap();
    let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext).unwrap();

    let receipt_envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id: peer_id,
        sender_device_id: peer_device_a,
        recipient_id: local_user_id,
        recipient_device_id: Some(local_device_id),
        created_at: Utc::now(),
        event_type: EventType::DeliveryReceipt,
        notify: false,
        ciphertext,
    };

    inbox
        .handle(Uuid::new_v4().to_string(), receipt_envelope)
        .await;

    let entry_a = outbox_repository
        .get_by_message_recipient_device(message_id, peer_id, peer_device_a)
        .await
        .unwrap();
    assert_eq!(entry_a.status, OutboxStatus::Delivered);

    let entry_b = outbox_repository
        .get_by_message_recipient_device(message_id, peer_id, peer_device_b)
        .await
        .unwrap();
    assert_eq!(entry_b.status, OutboxStatus::Pending);
}

#[tokio::test]
async fn an_envelope_addressed_to_another_device_is_dropped() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let local_user_id = Uuid::new_v4();
    let local_device_id = Uuid::new_v4();
    let sender_id = Uuid::new_v4();
    let sender_device_id = Uuid::new_v4();

    seed_user(&db, local_user_id).await;
    seed_user(&db, sender_id).await;
    seed_chat(&db, chat_id).await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository, SecretKey::default()));
    exchanger
        .generate_local_key(chat_id, local_user_id, local_device_id)
        .await
        .unwrap();
    exchanger
        .generate_local_key(chat_id, sender_id, sender_device_id)
        .await
        .unwrap();

    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let (identity_state, session_state) = registered(local_user_id, local_device_id);
    let (
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
    ) = inbox_support(&db, &identity_state, &exchanger, &outbox_repository);

    let inbox = Inbox::new(
        Arc::new(NoopSubscription),
        message_repository.clone(),
        outbox_repository,
        exchanger,
        Arc::new(ConnectivityState::default()),
        identity_state,
        session_state,
        facade_event_bus(),
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
    );

    let message_id = Uuid::new_v4();
    let payload = MessageWirePayload {
        message_id,
        reply_to_id: None,
        available_to: None,
        content: MessageContent {
            id: Uuid::new_v4(),
            text: "not for you".to_string(),
            attachments: Vec::new(),
            reactions: Vec::new(),
        },
    };
    let plaintext = serde_json::to_vec(&payload).unwrap();
    let ciphertext = plaintext;

    let envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id,
        sender_device_id,
        recipient_id: local_user_id,
        recipient_device_id: Some(Uuid::new_v4()),
        created_at: Utc::now(),
        event_type: EventType::Message,
        notify: true,
        ciphertext,
    };

    inbox.handle(Uuid::new_v4().to_string(), envelope).await;

    let result = message_repository.get(message_id).await;
    assert!(
        result.is_err(),
        "an envelope addressed to another device must not be processed"
    );
}

#[tokio::test]
async fn an_unregistered_inbox_ignores_envelopes() {
    use client::adapters::out::persistance::entities::message;
    use sea_orm::EntityTrait;

    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let sender_id = Uuid::new_v4();

    seed_user(&db, sender_id).await;
    seed_chat(&db, chat_id).await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository, SecretKey::default()));
    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let identity_state = Arc::new(IdentityState::new(IdentityStatus::Unregistered));
    let (
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
    ) = inbox_support(&db, &identity_state, &exchanger, &outbox_repository);

    let inbox = Inbox::new(
        Arc::new(NoopSubscription),
        message_repository,
        outbox_repository,
        exchanger,
        Arc::new(ConnectivityState::default()),
        identity_state,
        Arc::new(SessionState::default()),
        facade_event_bus(),
        peer_directory,
        invitation_repository,
        chat_repository,
        user_repository,
        device_keys,
        key_repository_for_inbox,
        inviter,
    );

    inbox
        .handle(
            "token".to_string(),
            EncryptedEnvelope {
                id: Uuid::new_v4(),
                chat_id,
                sender_id,
                sender_device_id: Uuid::new_v4(),
                recipient_id: Uuid::new_v4(),
                recipient_device_id: None,
                created_at: Utc::now(),
                event_type: EventType::Message,
                notify: true,
                ciphertext: vec![0u8; 16],
            },
        )
        .await;

    let rows = message::Entity::find().all(db.as_ref()).await.unwrap();
    assert!(
        rows.is_empty(),
        "an unregistered inbox must not persist anything"
    );
}
