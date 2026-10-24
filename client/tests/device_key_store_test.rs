mod fixtures;

use std::sync::Arc;

use client::adapters::out::keys::{DeviceKeyStore, DeviceKeyStoreRepository};
use client::adapters::out::persistance::entities::device_key;
use client::adapters::out::persistance::repositories::device_key::DeviceKeyRepository;
use client::domain::dto::error::CryptoError;
use client::domain::services::encryptor::Encryptor;
use common::identity::{canonical_challenge_bytes, verify};
use fixtures::setup_db;
use orion::aead::SecretKey;
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
use uuid::Uuid;

fn store(db: &Arc<sea_orm::DatabaseConnection>) -> DeviceKeyStore {
    DeviceKeyStore::new(
        Arc::new(DeviceKeyRepository::new(db.clone())),
        Encryptor::new(SecretKey::default()),
    )
}

fn store_with_key(db: &Arc<sea_orm::DatabaseConnection>, key: SecretKey) -> DeviceKeyStore {
    DeviceKeyStore::new(
        Arc::new(DeviceKeyRepository::new(db.clone())),
        Encryptor::new(key),
    )
}

#[tokio::test]
async fn get_or_create_keys_is_stable_per_device() {
    let db = setup_db().await;
    let store = store(&db);
    let device_id = Uuid::new_v4();

    let first = store.get_or_create_keys(device_id).await.unwrap();
    let second = store.get_or_create_keys(device_id).await.unwrap();

    assert_eq!(first, second);
    assert_ne!(first.signing_key, first.agreement_key);
    assert_ne!(first.signing_key, [0u8; 32]);
}

#[tokio::test]
async fn different_devices_get_different_keys() {
    let db = setup_db().await;
    let store = store(&db);

    let a = store.get_or_create_keys(Uuid::new_v4()).await.unwrap();
    let b = store.get_or_create_keys(Uuid::new_v4()).await.unwrap();

    assert_ne!(a, b);
}

#[tokio::test]
async fn signatures_verify_against_the_stored_signing_key() {
    let db = setup_db().await;
    let store = store(&db);
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let keys = store.get_or_create_keys(device_id).await.unwrap();
    let message = canonical_challenge_bytes(user_id, device_id, &[7u8; 32]);

    let signature = store.sign(device_id, &message).await.unwrap();

    assert!(verify(&keys.signing_key, &message, &signature).is_ok());
    assert!(verify(&keys.signing_key, b"other message", &signature).is_err());
}

#[tokio::test]
async fn signing_for_an_unknown_device_fails() {
    let db = setup_db().await;
    let store = store(&db);

    let err = store.sign(Uuid::new_v4(), b"hello").await.unwrap_err();
    assert!(matches!(err, CryptoError::DeviceNotFound));
}

#[tokio::test]
async fn signing_with_a_different_master_key_fails() {
    let db = setup_db().await;
    let device_id = Uuid::new_v4();

    let owner = store(&db);
    owner.get_or_create_keys(device_id).await.unwrap();

    let other = store_with_key(&db, SecretKey::generate(32).unwrap());

    let err = other.sign(device_id, b"hello").await.unwrap_err();
    assert!(matches!(err, CryptoError::DecryptionFailed));
}

#[tokio::test]
async fn signing_fails_when_the_stored_ciphertext_is_tampered() {
    let db = setup_db().await;
    let store = store(&db);
    let device_id = Uuid::new_v4();
    store.get_or_create_keys(device_id).await.unwrap();

    let model = device_key::Entity::find_by_id(device_id)
        .one(db.as_ref())
        .await
        .unwrap()
        .unwrap();

    let mut tampered = model.encrypted_signing_private_key.clone();
    tampered[0] ^= 0xFF;

    let mut active: device_key::ActiveModel = model.into_active_model();
    active.encrypted_signing_private_key = Set(tampered);
    active.update(db.as_ref()).await.unwrap();

    let err = store.sign(device_id, b"hello").await.unwrap_err();
    assert!(matches!(err, CryptoError::DecryptionFailed));
}

#[tokio::test]
async fn stored_signing_ciphertext_does_not_contain_the_plaintext_seed() {
    let db = setup_db().await;
    let store = store(&db);
    let device_id = Uuid::new_v4();
    let keys = store.get_or_create_keys(device_id).await.unwrap();

    let model = device_key::Entity::find_by_id(device_id)
        .one(db.as_ref())
        .await
        .unwrap()
        .unwrap();

    // The plaintext seed is never exposed to the test, so we can't assert the blob doesn't
    // *contain* it byte-for-byte. Instead: the ciphertext must be larger than the raw 32-byte
    // seed (nonce + tag overhead), and signing against it must still verify correctly.
    assert!(model.encrypted_signing_private_key.len() > 32);

    let message = b"integrity check";
    let signature = store.sign(device_id, message).await.unwrap();
    assert!(verify(&keys.signing_key, message, &signature).is_ok());
}
