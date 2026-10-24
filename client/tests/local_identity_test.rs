mod fixtures;

use std::sync::Arc;

use client::adapters::out::keys::DeviceKeyStore;
use client::adapters::out::persistance::entities::{chat, device_key, local_identity, user};
use client::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use client::adapters::out::persistance::repositories::local_identity::LocalIdentityRepository;
use client::domain::dto::errors::IdentityStoreError;
use client::domain::services::encryptor::Encryptor;
use fixtures::setup_db;
use orion::aead::SecretKey;
use sea_orm::EntityTrait;
use uuid::Uuid;

fn repository(db: &Arc<sea_orm::DatabaseConnection>) -> LocalIdentityRepository {
    let device_keys = Arc::new(DeviceKeyStore::new(
        Arc::new(DeviceKeyRepository::new(db.clone())),
        Encryptor::new(SecretKey::default()),
    ));
    LocalIdentityRepository::new(db.clone(), device_keys)
}

#[tokio::test]
async fn get_returns_none_before_create() {
    let db = setup_db().await;
    assert!(repository(&db).get().await.unwrap().is_none());
}

#[tokio::test]
async fn create_persists_the_identity_and_a_local_user_row() {
    let db = setup_db().await;
    let repo = repository(&db);
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let created = repo.create(user_id, device_id).await.unwrap();

    assert_eq!(created.user_id, user_id);
    assert_eq!(created.device_id, device_id);
    assert_eq!(created.agreement_public_key.len(), 32);
    assert_eq!(created.signing_public_key.len(), 32);

    let loaded = repo.get().await.unwrap().unwrap();
    assert_eq!(loaded, created);

    let local_user = user::Entity::find_by_id(user_id)
        .one(db.as_ref())
        .await
        .unwrap()
        .expect("local users row should exist");
    assert!(local_user.nickname.starts_with("user-"));
}

#[tokio::test]
async fn create_does_not_overwrite_an_existing_user_row() {
    let db = setup_db().await;
    let repo = repository(&db);
    let user_id = Uuid::new_v4();
    fixtures::seed_user(&db, user_id).await;

    repo.create(user_id, Uuid::new_v4()).await.unwrap();

    let local_user = user::Entity::find_by_id(user_id)
        .one(db.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(local_user.nickname, user_id.to_string());
}

#[tokio::test]
async fn create_twice_fails() {
    let db = setup_db().await;
    let repo = repository(&db);
    repo.create(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();

    let err = repo
        .create(Uuid::new_v4(), Uuid::new_v4())
        .await
        .unwrap_err();
    assert!(matches!(err, IdentityStoreError::AlreadyExists));
}

#[tokio::test]
async fn remove_clears_identity_and_device_keys() {
    let db = setup_db().await;
    let repo = repository(&db);
    repo.create(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();

    repo.remove().await.unwrap();

    assert!(repo.get().await.unwrap().is_none());
    assert_eq!(
        local_identity::Entity::find()
            .all(db.as_ref())
            .await
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        device_key::Entity::find()
            .all(db.as_ref())
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn remove_on_an_empty_database_is_ok() {
    let db = setup_db().await;
    let repo = repository(&db);

    assert!(repo.remove().await.is_ok());
    assert!(repo.get().await.unwrap().is_none());
}

#[tokio::test]
async fn create_after_remove_succeeds_again() {
    let db = setup_db().await;
    let repo = repository(&db);
    repo.create(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
    repo.remove().await.unwrap();

    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let created = repo.create(user_id, device_id).await.unwrap();

    assert_eq!(created.user_id, user_id);
    assert_eq!(created.device_id, device_id);

    let loaded = repo.get().await.unwrap().unwrap();
    assert_eq!(loaded, created);
}

#[tokio::test]
async fn remove_keeps_users_and_chats() {
    let db = setup_db().await;
    let repo = repository(&db);
    let user_id = Uuid::new_v4();
    let chat_id = Uuid::new_v4();

    repo.create(user_id, Uuid::new_v4()).await.unwrap();
    fixtures::seed_chat(&db, chat_id).await;

    repo.remove().await.unwrap();

    let local_user = user::Entity::find_by_id(user_id)
        .one(db.as_ref())
        .await
        .unwrap();
    assert!(
        local_user.is_some(),
        "local users row should survive remove"
    );

    let persisted_chat = chat::Entity::find_by_id(chat_id)
        .one(db.as_ref())
        .await
        .unwrap();
    assert!(persisted_chat.is_some(), "chat row should survive remove");
}
