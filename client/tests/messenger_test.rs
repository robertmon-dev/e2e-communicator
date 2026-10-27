mod fixtures;

use std::collections::BTreeMap;
use std::sync::Arc;

use orion::aead::SecretKey;
use orion::hazardous::ecc::x25519::{PrivateKey as X25519PrivateKey, PublicKey as X25519PublicKey};
use uuid::Uuid;

use client::adapters::out::persistance::repositories::chat::ChatRepository;
use client::adapters::out::persistance::repositories::chat::create::Create as ChatCreate;
use client::adapters::out::persistance::repositories::key::KeyRepository;
use client::adapters::out::persistance::repositories::message::MessageRepository;
use client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use client::domain::dto::chat::{Chat, ChatType};
use client::domain::dto::permission::MemberRole;
use client::domain::services::exchanger::Exchanger;
use client::domain::services::messenger::Messenger;
use fixtures::{seed_user, setup_db};

fn random_x25519_public_key() -> Vec<u8> {
    let private_key = X25519PrivateKey::generate();
    let public_key = X25519PublicKey::try_from(&private_key).unwrap();
    public_key.to_bytes().to_vec()
}

async fn seed_group_chat(chat_repository: &ChatRepository, chat_id: Uuid, members: &[Uuid]) {
    let mut roles = BTreeMap::new();
    for member_id in members {
        roles.insert(*member_id, MemberRole::Member);
    }

    let chat = Chat::new(ChatType::Group, None, roles, BTreeMap::new(), None, true);
    let chat = Chat {
        id: chat_id,
        ..chat
    };

    chat_repository.create(chat).await.unwrap();
}

#[tokio::test]
async fn send_addresses_one_envelope_per_known_peer_device() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let author_id = Uuid::new_v4();
    let author_device_id = Uuid::new_v4();
    let peer_id = Uuid::new_v4();
    let peer_device_a = Uuid::new_v4();
    let peer_device_b = Uuid::new_v4();

    seed_user(&db, author_id).await;
    seed_user(&db, peer_id).await;

    let chat_repository = Arc::new(ChatRepository::new(db.clone()));
    seed_group_chat(&chat_repository, chat_id, &[author_id, peer_id]).await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository.clone(), SecretKey::default()));

    exchanger
        .generate_local_key(chat_id, author_id, author_device_id)
        .await
        .unwrap();
    exchanger
        .store_peer_key(chat_id, peer_id, peer_device_a, random_x25519_public_key())
        .await
        .unwrap();
    exchanger
        .store_peer_key(chat_id, peer_id, peer_device_b, random_x25519_public_key())
        .await
        .unwrap();

    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let messenger = Messenger::new(
        message_repository,
        chat_repository,
        outbox_repository.clone(),
        key_repository,
        exchanger,
    );

    let (message, envelopes) = messenger
        .send(
            chat_id,
            author_id,
            author_device_id,
            "hello both devices".to_string(),
            None,
            None,
            Vec::new(),
            None,
        )
        .await
        .unwrap();

    assert_eq!(envelopes.len(), 2);
    let mut recipient_devices: Vec<Uuid> = envelopes
        .iter()
        .map(|e| e.recipient_device_id.unwrap())
        .collect();
    recipient_devices.sort();
    let mut expected = vec![peer_device_a, peer_device_b];
    expected.sort();
    assert_eq!(recipient_devices, expected);

    let entry_a = outbox_repository
        .get_by_message_recipient_device(message.id, peer_id, peer_device_a)
        .await
        .unwrap();
    assert_eq!(entry_a.recipient_device_id, peer_device_a);

    let entry_b = outbox_repository
        .get_by_message_recipient_device(message.id, peer_id, peer_device_b)
        .await
        .unwrap();
    assert_eq!(entry_b.recipient_device_id, peer_device_b);
}

#[tokio::test]
async fn send_skips_a_member_with_no_known_device_keys_without_failing() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let author_id = Uuid::new_v4();
    let author_device_id = Uuid::new_v4();
    let peer_with_keys = Uuid::new_v4();
    let peer_with_keys_device = Uuid::new_v4();
    let peer_without_keys = Uuid::new_v4();

    seed_user(&db, author_id).await;
    seed_user(&db, peer_with_keys).await;
    seed_user(&db, peer_without_keys).await;

    let chat_repository = Arc::new(ChatRepository::new(db.clone()));
    seed_group_chat(
        &chat_repository,
        chat_id,
        &[author_id, peer_with_keys, peer_without_keys],
    )
    .await;

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository.clone(), SecretKey::default()));

    exchanger
        .generate_local_key(chat_id, author_id, author_device_id)
        .await
        .unwrap();
    exchanger
        .store_peer_key(
            chat_id,
            peer_with_keys,
            peer_with_keys_device,
            random_x25519_public_key(),
        )
        .await
        .unwrap();

    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));

    let messenger = Messenger::new(
        message_repository,
        chat_repository,
        outbox_repository.clone(),
        key_repository,
        exchanger,
    );

    let (message, envelopes) = messenger
        .send(
            chat_id,
            author_id,
            author_device_id,
            "hello, partially known group".to_string(),
            None,
            None,
            Vec::new(),
            None,
        )
        .await
        .unwrap();

    assert_eq!(envelopes.len(), 1);
    assert_eq!(
        envelopes[0].recipient_device_id,
        Some(peer_with_keys_device)
    );

    let entry = outbox_repository
        .get_by_message_recipient_device(message.id, peer_with_keys, peer_with_keys_device)
        .await
        .unwrap();
    assert_eq!(entry.recipient_id, peer_with_keys);

    let missing = outbox_repository
        .get_by_message_recipient_device(message.id, peer_without_keys, Uuid::new_v4())
        .await;
    assert!(missing.is_err());
}
