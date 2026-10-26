mod fixtures;

use fixtures::setup_db;

use client::adapters::out::persistance::repositories::user_identity::UserIdentityRepository;
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
