mod fixtures;

use chrono::Utc;
use uuid::Uuid;

use client::adapters::out::persistance::repositories::key::KeyRepository;
use client::adapters::out::persistance::repositories::key::create::Create;
use client::domain::dto::key::Key;
use fixtures::{seed_chat, seed_user, setup_db};

fn key_for(chat_id: Uuid, user_id: Uuid, device_id: Uuid, public_key: Vec<u8>) -> Key {
    let now = Utc::now();
    Key::new(
        Uuid::new_v4(),
        chat_id,
        user_id,
        device_id,
        public_key,
        None,
        now,
        now,
    )
}

#[tokio::test]
async fn two_devices_of_the_same_user_coexist_in_a_chat() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let device_a = Uuid::new_v4();
    let device_b = Uuid::new_v4();

    seed_user(&db, user_id).await;
    seed_chat(&db, chat_id).await;

    let repository = KeyRepository::new(db.clone());
    repository
        .create(key_for(chat_id, user_id, device_a, vec![1; 32]))
        .await
        .unwrap();
    repository
        .create(key_for(chat_id, user_id, device_b, vec![2; 32]))
        .await
        .unwrap();

    let keys = repository
        .list_by_chat_and_user(chat_id, user_id)
        .await
        .unwrap();
    assert_eq!(keys.len(), 2);

    let key_a = repository
        .get_by_chat_user_device(chat_id, user_id, device_a)
        .await
        .unwrap();
    assert_eq!(key_a.public_key, vec![1; 32]);

    let key_b = repository
        .get_by_chat_user_device(chat_id, user_id, device_b)
        .await
        .unwrap();
    assert_eq!(key_b.public_key, vec![2; 32]);
}

#[tokio::test]
async fn a_missing_device_key_is_an_error() {
    let db = setup_db().await;
    let chat_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    seed_user(&db, user_id).await;
    seed_chat(&db, chat_id).await;

    let repository = KeyRepository::new(db.clone());

    let result = repository
        .get_by_chat_user_device(chat_id, user_id, Uuid::new_v4())
        .await;
    assert!(result.is_err());
}
