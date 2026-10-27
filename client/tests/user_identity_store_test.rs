mod fixtures;

use fixtures::setup_db;

use client::adapters::out::keys::{UserIdentityStore, UserIdentityStoreRepository};
use client::adapters::out::persistance::entities::user_identity;
use client::adapters::out::persistance::repositories::user_identity::UserIdentityRepository;
use client::domain::dto::errors::IdentityStoreError;
use client::domain::services::encryptor::Encryptor;
use common::identity::{DevicePublicKeys, canonical_device_cert_bytes, verify};
use orion::aead::SecretKey;
use sea_orm::EntityTrait;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn stores_and_reads_back_user_identity() {
    let db = setup_db().await;
    let repository = UserIdentityRepository::new(db.clone());
    let user_id = Uuid::new_v4();

    assert!(repository.get().await.unwrap().is_none());

    let stored = repository
        .create(user_id, vec![3u8; 32], vec![9u8; 64])
        .await
        .unwrap();

    assert_eq!(stored.user_id, user_id);
    let read_back = repository.get().await.unwrap().unwrap();
    assert_eq!(read_back.signing_public_key, vec![3u8; 32]);
    assert_eq!(read_back.encrypted_signing_private_key, vec![9u8; 64]);
}

#[tokio::test]
async fn remove_clears_user_identity() {
    let db = setup_db().await;
    let repository = UserIdentityRepository::new(db.clone());
    repository
        .create(Uuid::new_v4(), vec![1u8; 32], vec![2u8; 64])
        .await
        .unwrap();

    repository.remove().await.unwrap();

    assert!(repository.get().await.unwrap().is_none());
}

#[tokio::test]
async fn create_twice_is_rejected_even_for_a_different_user() {
    let db = setup_db().await;
    let repository = UserIdentityRepository::new(db.clone());
    repository
        .create(Uuid::new_v4(), vec![1u8; 32], vec![2u8; 64])
        .await
        .unwrap();

    let result = repository
        .create(Uuid::new_v4(), vec![3u8; 32], vec![4u8; 64])
        .await;

    assert!(matches!(result, Err(IdentityStoreError::AlreadyExists)));
    assert_eq!(
        user_identity::Entity::find()
            .all(db.as_ref())
            .await
            .unwrap()
            .len(),
        1,
        "tabela trzyma dokladnie jedna tozsamosc"
    );
}

#[tokio::test]
async fn remove_on_an_empty_database_is_ok() {
    let db = setup_db().await;
    let repository = UserIdentityRepository::new(db.clone());

    assert!(repository.remove().await.is_ok());
}

fn user_identity_store(db: &Arc<sea_orm::DatabaseConnection>) -> UserIdentityStore {
    UserIdentityStore::new(
        Arc::new(UserIdentityRepository::new(db.clone())),
        Encryptor::new(SecretKey::default()),
    )
}

#[tokio::test]
async fn mints_cert_verifiable_under_generated_user_key() {
    let db = setup_db().await;
    let store = user_identity_store(&db);
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let device_keys = DevicePublicKeys {
        signing_key: [5u8; 32],
        agreement_key: [6u8; 32],
    };

    let user_public = store.get_or_create(user_id).await.unwrap();
    let cert = store
        .certify_device(user_id, device_id, &device_keys)
        .await
        .unwrap();

    let message = canonical_device_cert_bytes(user_id, device_id, &device_keys);
    assert!(verify(&user_public, &message, &cert).is_ok());
}

#[tokio::test]
async fn get_or_create_is_idempotent_and_seed_is_exportable() {
    let db = setup_db().await;
    let store = user_identity_store(&db);
    let user_id = Uuid::new_v4();

    let first = store.get_or_create(user_id).await.unwrap();
    let second = store.get_or_create(user_id).await.unwrap();
    assert_eq!(first, second);

    let seed = store.export_seed().await.unwrap();
    assert_eq!(common::identity::signing_public_key(&seed), first);
}

#[tokio::test]
async fn import_seed_adopts_existing_user_key() {
    let db = setup_db().await;
    let store = user_identity_store(&db);
    let user_id = Uuid::new_v4();
    let seed = common::identity::generate_signing_seed().unwrap();
    let expected = common::identity::signing_public_key(&seed);

    let adopted = store.import_seed(user_id, &seed).await.unwrap();

    assert_eq!(adopted, expected);
    assert_eq!(store.get_or_create(user_id).await.unwrap(), expected);
}

#[tokio::test]
async fn export_seed_without_identity_is_an_error() {
    let db = setup_db().await;
    let store = user_identity_store(&db);

    assert!(store.export_seed().await.is_err());
}
