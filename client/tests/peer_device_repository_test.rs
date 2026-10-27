mod fixtures;

use client::adapters::out::persistance::repositories::peer_device::{
    PeerDeviceRecord, PeerDeviceRepository,
};
use fixtures::{seed_user, setup_db};
use uuid::Uuid;

fn record(user_id: Uuid, device_id: Uuid, signing_public_key: Vec<u8>) -> PeerDeviceRecord {
    PeerDeviceRecord {
        user_id,
        device_id,
        signing_public_key,
        agreement_public_key: vec![9u8; 32],
        device_cert: vec![7u8; 64],
    }
}

#[tokio::test]
async fn upsert_many_overwrites_rather_than_duplicates() {
    let db = setup_db().await;
    let user_id = Uuid::new_v4();
    seed_user(&db, user_id).await;
    let repo = PeerDeviceRepository::new(db.clone());
    let device_id = Uuid::new_v4();

    repo.upsert_many(vec![record(user_id, device_id, vec![1u8; 32])])
        .await
        .unwrap();
    repo.upsert_many(vec![record(user_id, device_id, vec![2u8; 32])])
        .await
        .unwrap();

    let rows = repo.list_by_user(user_id).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].signing_public_key, vec![2u8; 32]);
}

#[tokio::test]
async fn upsert_many_with_empty_input_is_a_noop() {
    let db = setup_db().await;
    let user_id = Uuid::new_v4();
    seed_user(&db, user_id).await;
    let repo = PeerDeviceRepository::new(db.clone());

    repo.upsert_many(vec![]).await.unwrap();

    assert!(repo.list_by_user(user_id).await.unwrap().is_empty());
}

#[tokio::test]
async fn pin_identity_is_readable_and_starts_unverified() {
    let db = setup_db().await;
    let user_id = Uuid::new_v4();
    seed_user(&db, user_id).await;
    let repo = PeerDeviceRepository::new(db.clone());

    repo.pin_identity(user_id, vec![1u8; 32]).await.unwrap();

    assert_eq!(
        repo.pinned_identity(user_id).await.unwrap(),
        Some(vec![1u8; 32])
    );
    assert_eq!(repo.verified_at(user_id).await.unwrap(), None);

    repo.mark_verified(user_id).await.unwrap();
    assert!(repo.verified_at(user_id).await.unwrap().is_some());
}

#[tokio::test]
async fn pinning_a_different_identity_clears_prior_human_verification() {
    let db = setup_db().await;
    let user_id = Uuid::new_v4();
    seed_user(&db, user_id).await;
    let repo = PeerDeviceRepository::new(db.clone());

    repo.pin_identity(user_id, vec![1u8; 32]).await.unwrap();
    repo.mark_verified(user_id).await.unwrap();
    assert!(repo.verified_at(user_id).await.unwrap().is_some());

    repo.pin_identity(user_id, vec![2u8; 32]).await.unwrap();

    assert_eq!(
        repo.pinned_identity(user_id).await.unwrap(),
        Some(vec![2u8; 32])
    );
    assert_eq!(
        repo.verified_at(user_id).await.unwrap(),
        None,
        "a changed identity must invalidate any prior human verification"
    );
}

#[tokio::test]
async fn pinning_the_same_identity_again_keeps_human_verification() {
    let db = setup_db().await;
    let repository = PeerDeviceRepository::new(db.clone());
    let user_id = Uuid::new_v4();
    seed_user(&db, user_id).await;

    repository
        .pin_identity(user_id, vec![9u8; 32])
        .await
        .unwrap();
    repository.mark_verified(user_id).await.unwrap();

    repository
        .pin_identity(user_id, vec![9u8; 32])
        .await
        .unwrap();

    assert!(
        repository.verified_at(user_id).await.unwrap().is_some(),
        "powtorne przypiecie tego samego klucza nie moze czyscic verified_at"
    );
}
