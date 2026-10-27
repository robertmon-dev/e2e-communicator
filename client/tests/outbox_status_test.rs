mod fixtures;

use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use client::adapters::out::persistance::repositories::outbox::create::Create;
use client::adapters::out::persistance::repositories::outbox::update::Update;
use client::domain::dto::outbox::{OutboxEntry, OutboxStatus};
use common::envelope::EventType;
use fixtures::{seed_chat, seed_message, seed_user, setup_db};

#[test]
fn outbox_status_round_trips_through_i32() {
    let statuses = [
        OutboxStatus::Pending,
        OutboxStatus::Sent,
        OutboxStatus::Failed,
        OutboxStatus::Delivered,
    ];

    for status in statuses {
        let raw: i32 = status.into();
        let parsed = OutboxStatus::try_from(raw).unwrap();
        assert_eq!(parsed, status);
    }
}

fn entry_without_message(
    chat_id: Uuid,
    sender_id: Uuid,
    recipient_id: Uuid,
    recipient_device_id: Uuid,
) -> OutboxEntry {
    let now = Utc::now();

    OutboxEntry::new(
        Uuid::new_v4(),
        None,
        chat_id,
        sender_id,
        recipient_id,
        recipient_device_id,
        EventType::Message,
        vec![1, 2, 3],
        OutboxStatus::Pending,
        0,
        None,
        None,
        now,
        now,
    )
}

#[tokio::test]
async fn an_outbox_entry_without_a_message_id_persists_and_keeps_its_device() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let sender_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    let recipient_device_id = Uuid::new_v4();

    seed_user(&db, sender_id).await;
    seed_user(&db, recipient_id).await;
    seed_chat(&db, chat_id).await;

    let outbox = Arc::new(OutboxRepository::new(db.clone()));
    let entry = entry_without_message(chat_id, sender_id, recipient_id, recipient_device_id);
    let entry_id = entry.id;

    outbox.create_many(vec![entry]).await.unwrap();

    let stored = outbox.get(entry_id).await.unwrap();
    assert_eq!(stored.message_id, None);
    assert_eq!(stored.recipient_device_id, recipient_device_id);
}

#[tokio::test]
async fn a_receipt_matches_the_correct_device_among_several() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let sender_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    let device_a = Uuid::new_v4();
    let device_b = Uuid::new_v4();
    let message_id = Uuid::new_v4();

    seed_user(&db, sender_id).await;
    seed_user(&db, recipient_id).await;
    seed_chat(&db, chat_id).await;
    seed_message(&db, message_id, sender_id, chat_id).await;

    let outbox = Arc::new(OutboxRepository::new(db.clone()));

    let now = Utc::now();
    let entry_a = OutboxEntry::new(
        Uuid::new_v4(),
        Some(message_id),
        chat_id,
        sender_id,
        recipient_id,
        device_a,
        EventType::Message,
        vec![1],
        OutboxStatus::Pending,
        0,
        None,
        None,
        now,
        now,
    );
    let entry_b = OutboxEntry::new(
        Uuid::new_v4(),
        Some(message_id),
        chat_id,
        sender_id,
        recipient_id,
        device_b,
        EventType::Message,
        vec![2],
        OutboxStatus::Pending,
        0,
        None,
        None,
        now,
        now,
    );

    outbox
        .create_many(vec![entry_a.clone(), entry_b.clone()])
        .await
        .unwrap();

    let matched = outbox
        .get_by_message_recipient_device(message_id, recipient_id, device_b)
        .await
        .unwrap();
    assert_eq!(matched.id, entry_b.id);
    assert_eq!(matched.recipient_device_id, device_b);

    outbox.mark_delivered(matched.id).await.unwrap();

    assert!(
        outbox
            .any_delivered_for(message_id, recipient_id)
            .await
            .unwrap()
    );

    let still_pending = outbox.get(entry_a.id).await.unwrap();
    assert_eq!(still_pending.status, OutboxStatus::Pending);
}
