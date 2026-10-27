use client::domain::dto::chat_key::ChatKeyBundle;
use common::identity::{generate_signing_seed, sign, signing_public_key};
use uuid::Uuid;

fn bundle_signed_with(
    seed: &[u8; 32],
    chat_id: Uuid,
    user_id: Uuid,
    device_id: Uuid,
    chat_public_key: [u8; 32],
) -> ChatKeyBundle {
    ChatKeyBundle::signed(chat_id, user_id, device_id, chat_public_key, |message| {
        sign(seed, message)
    })
}

#[test]
fn bundle_verifies_against_the_signing_devices_own_key() {
    let seed = generate_signing_seed().unwrap();
    let device_key = signing_public_key(&seed);

    let bundle = bundle_signed_with(
        &seed,
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        [7u8; 32],
    );

    assert!(bundle.verify(&device_key).is_ok());
}

#[test]
fn bundle_does_not_verify_against_a_different_devices_key() {
    let seed = generate_signing_seed().unwrap();
    let other_seed = generate_signing_seed().unwrap();
    let other_device_key = signing_public_key(&other_seed);

    let bundle = bundle_signed_with(
        &seed,
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        [7u8; 32],
    );

    assert!(bundle.verify(&other_device_key).is_err());
}

#[test]
fn tampering_with_chat_public_key_after_signing_invalidates_the_signature() {
    let seed = generate_signing_seed().unwrap();
    let device_key = signing_public_key(&seed);

    let mut bundle = bundle_signed_with(
        &seed,
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        [7u8; 32],
    );
    bundle.chat_public_key = [9u8; 32];

    assert!(bundle.verify(&device_key).is_err());
}

#[test]
fn tampering_with_device_id_after_signing_invalidates_the_signature() {
    let seed = generate_signing_seed().unwrap();
    let device_key = signing_public_key(&seed);

    let mut bundle = bundle_signed_with(
        &seed,
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        [7u8; 32],
    );
    bundle.device_id = Uuid::new_v4();

    assert!(bundle.verify(&device_key).is_err());
}
