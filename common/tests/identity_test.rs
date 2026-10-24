use common::identity::{
    DevicePublicKeys, IdentityError, canonical_challenge_bytes, canonical_device_bytes,
    decode_token, encode_token, generate_signing_seed, random_bytes, sign, signing_public_key,
    verify,
};
use uuid::Uuid;

fn keys(seed: &[u8; 32]) -> DevicePublicKeys {
    DevicePublicKeys {
        signing_key: signing_public_key(seed),
        agreement_key: [9u8; 32],
    }
}

#[test]
fn sign_and_verify_round_trip() {
    let seed = generate_signing_seed().unwrap();
    let public = signing_public_key(&seed);
    let message = canonical_device_bytes(Uuid::new_v4(), Uuid::new_v4(), &keys(&seed));

    let signature = sign(&seed, &message);

    assert!(verify(&public, &message, &signature).is_ok());
}

#[test]
fn verify_fails_when_any_canonical_field_changes() {
    let seed = generate_signing_seed().unwrap();
    let public = signing_public_key(&seed);
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let signature = sign(
        &seed,
        &canonical_device_bytes(user_id, device_id, &keys(&seed)),
    );

    let other_user = canonical_device_bytes(Uuid::new_v4(), device_id, &keys(&seed));
    let other_device = canonical_device_bytes(user_id, Uuid::new_v4(), &keys(&seed));
    let mut other_keys = keys(&seed);
    other_keys.agreement_key = [1u8; 32];
    let other_agreement = canonical_device_bytes(user_id, device_id, &other_keys);

    assert_eq!(
        verify(&public, &other_user, &signature),
        Err(IdentityError::InvalidSignature)
    );
    assert_eq!(
        verify(&public, &other_device, &signature),
        Err(IdentityError::InvalidSignature)
    );
    assert_eq!(
        verify(&public, &other_agreement, &signature),
        Err(IdentityError::InvalidSignature)
    );
}

#[test]
fn verify_fails_with_the_wrong_key() {
    let seed = generate_signing_seed().unwrap();
    let other_seed = generate_signing_seed().unwrap();
    let message = canonical_challenge_bytes(Uuid::new_v4(), Uuid::new_v4(), &[3u8; 32]);
    let signature = sign(&seed, &message);

    assert_eq!(
        verify(&signing_public_key(&other_seed), &message, &signature),
        Err(IdentityError::InvalidSignature)
    );
}

#[test]
fn device_and_challenge_bytes_are_domain_separated() {
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let device = canonical_device_bytes(
        user_id,
        device_id,
        &DevicePublicKeys {
            signing_key: [0u8; 32],
            agreement_key: [0u8; 32],
        },
    );
    let challenge = canonical_challenge_bytes(user_id, device_id, &[0u8; 32]);

    assert_ne!(device, challenge);
    assert!(device.starts_with(b"e2e-communicator/device/v1"));
    assert!(challenge.starts_with(b"e2e-communicator/challenge/v1"));
}

#[test]
fn device_public_keys_reject_wrong_lengths() {
    assert_eq!(
        DevicePublicKeys::from_slices(&[0u8; 31], &[0u8; 32]),
        Err(IdentityError::InvalidKey)
    );
    assert_eq!(
        DevicePublicKeys::from_slices(&[0u8; 32], &[0u8; 33]),
        Err(IdentityError::InvalidKey)
    );
    assert!(DevicePublicKeys::from_slices(&[0u8; 32], &[0u8; 32]).is_ok());
}

#[test]
fn tokens_round_trip_through_base64url_without_padding() {
    let bytes = random_bytes::<32>().unwrap();
    let token = encode_token(&bytes);

    assert!(!token.contains('='));
    assert!(!token.contains('+'));
    assert!(!token.contains('/'));
    assert_eq!(decode_token(&token).unwrap(), bytes.to_vec());
}

#[test]
fn random_bytes_are_not_constant() {
    let a = random_bytes::<32>().unwrap();
    let b = random_bytes::<32>().unwrap();
    assert_ne!(a, b);
}
