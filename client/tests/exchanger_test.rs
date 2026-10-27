mod fixtures;

use std::sync::Arc;

use orion::aead::SecretKey;
use orion::hazardous::ecc::x25519::{PrivateKey as X25519PrivateKey, PublicKey as X25519PublicKey};
use uuid::Uuid;

use client::adapters::out::persistance::repositories::key::KeyRepository;
use client::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};
use client::domain::services::exchanger::Exchanger;
use fixtures::{seed_chat, seed_user, setup_db};

fn random_x25519_public_key() -> Vec<u8> {
    let private_key = X25519PrivateKey::generate();
    let public_key = X25519PublicKey::try_from(&private_key).unwrap();
    public_key.to_bytes().to_vec()
}

#[tokio::test]
async fn two_devices_derive_the_same_shared_key_after_exchanging_peer_keys() {
    let db_a = setup_db().await;
    let db_b = setup_db().await;

    let chat_id = Uuid::new_v4();
    let user_a = Uuid::new_v4();
    let device_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    let device_b = Uuid::new_v4();

    seed_user(&db_a, user_a).await;
    seed_user(&db_a, user_b).await;
    seed_chat(&db_a, chat_id).await;

    seed_user(&db_b, user_a).await;
    seed_user(&db_b, user_b).await;
    seed_chat(&db_b, chat_id).await;

    let exchanger_a = Exchanger::new(
        Arc::new(KeyRepository::new(db_a.clone())),
        SecretKey::default(),
    );
    let exchanger_b = Exchanger::new(
        Arc::new(KeyRepository::new(db_b.clone())),
        SecretKey::default(),
    );

    let key_a = exchanger_a
        .generate_local_key(chat_id, user_a, device_a)
        .await
        .unwrap();
    let key_b = exchanger_b
        .generate_local_key(chat_id, user_b, device_b)
        .await
        .unwrap();

    exchanger_a
        .store_peer_key(chat_id, user_b, device_b, key_b.public_key.clone())
        .await
        .unwrap();
    exchanger_b
        .store_peer_key(chat_id, user_a, device_a, key_a.public_key.clone())
        .await
        .unwrap();

    let shared_a = exchanger_a
        .derive_shared_key(chat_id, (user_a, device_a), (user_b, device_b))
        .await
        .unwrap();
    let shared_b = exchanger_b
        .derive_shared_key(chat_id, (user_b, device_b), (user_a, device_a))
        .await
        .unwrap();

    let plaintext = b"shared secret round trip";
    let ciphertext = Encryptor::encrypt_with_derived_key(&shared_a, plaintext).unwrap();
    let decrypted = Encryptor::decrypt_with_derived_key(&shared_b, &ciphertext).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[tokio::test]
async fn two_devices_of_the_same_peer_yield_different_shared_keys() {
    let db = setup_db().await;

    let chat_id = Uuid::new_v4();
    let user_a = Uuid::new_v4();
    let device_a = Uuid::new_v4();
    let peer_id = Uuid::new_v4();
    let peer_device_1 = Uuid::new_v4();
    let peer_device_2 = Uuid::new_v4();

    seed_user(&db, user_a).await;
    seed_user(&db, peer_id).await;
    seed_chat(&db, chat_id).await;

    let exchanger = Exchanger::new(
        Arc::new(KeyRepository::new(db.clone())),
        SecretKey::default(),
    );

    exchanger
        .generate_local_key(chat_id, user_a, device_a)
        .await
        .unwrap();
    exchanger
        .store_peer_key(chat_id, peer_id, peer_device_1, random_x25519_public_key())
        .await
        .unwrap();
    exchanger
        .store_peer_key(chat_id, peer_id, peer_device_2, random_x25519_public_key())
        .await
        .unwrap();

    let shared_with_1 = exchanger
        .derive_shared_key(chat_id, (user_a, device_a), (peer_id, peer_device_1))
        .await
        .unwrap();
    let shared_with_2 = exchanger
        .derive_shared_key(chat_id, (user_a, device_a), (peer_id, peer_device_2))
        .await
        .unwrap();

    assert_ne!(
        shared_with_1.unprotected_as_bytes(),
        shared_with_2.unprotected_as_bytes()
    );
}
