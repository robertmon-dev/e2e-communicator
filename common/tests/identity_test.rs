use common::identity::{
    DevicePublicKeys, IdentityError, canonical_challenge_bytes, canonical_device_bytes,
    decode_token, decode_token_array, encode_token, fixed, generate_signing_seed, random_bytes,
    sign, signing_public_key, verify,
};
use uuid::Uuid;

fn keys(seed: &[u8; 32]) -> DevicePublicKeys {
    DevicePublicKeys {
        signing_key: signing_public_key(seed),
        agreement_key: [9u8; 32],
    }
}

fn decode_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "hex string must have an even length");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex digit"))
        .collect()
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
    let mut other_signing_keys = keys(&seed);
    other_signing_keys.signing_key = [2u8; 32];
    let other_signing = canonical_device_bytes(user_id, device_id, &other_signing_keys);

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
    assert_eq!(
        verify(&public, &other_signing, &signature),
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

#[test]
fn sign_and_verify_match_rfc8032_test_vector_1() {
    let seed: [u8; 32] =
        decode_hex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
            .try_into()
            .unwrap();
    let expected_public: [u8; 32] =
        decode_hex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
            .try_into()
            .unwrap();
    let expected_signature: [u8; 64] = decode_hex(concat!(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901",
        "555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    ))
    .try_into()
    .unwrap();

    let public = signing_public_key(&seed);
    assert_eq!(public, expected_public);

    let signature = sign(&seed, b"");
    assert_eq!(signature, expected_signature);
    assert!(verify(&public, b"", &signature).is_ok());
}

#[test]
fn verify_fails_when_signature_is_tampered() {
    let seed = generate_signing_seed().unwrap();
    let public = signing_public_key(&seed);
    let message = canonical_device_bytes(Uuid::new_v4(), Uuid::new_v4(), &keys(&seed));
    let mut signature = sign(&seed, &message);
    signature[0] ^= 0x01;

    assert_eq!(
        verify(&public, &message, &signature),
        Err(IdentityError::InvalidSignature)
    );
}

#[test]
fn decode_token_rejects_invalid_base64url() {
    assert_eq!(decode_token("AAAA="), Err(IdentityError::InvalidToken));
    assert_eq!(decode_token("a+b/"), Err(IdentityError::InvalidToken));
    assert_eq!(decode_token("AB"), Err(IdentityError::InvalidToken));
}

#[test]
fn decode_token_array_enforces_exact_length() {
    let short_token = encode_token(&[0u8; 16]);
    let exact_token = encode_token(&[7u8; 32]);

    assert_eq!(
        decode_token_array::<32>(&short_token),
        Err(IdentityError::InvalidToken)
    );
    assert_eq!(decode_token_array::<32>(&exact_token).unwrap(), [7u8; 32]);
}

#[test]
fn fixed_enforces_exact_length() {
    assert!(fixed::<32>(&[0u8; 32]).is_ok());
    assert_eq!(fixed::<32>(&[0u8; 31]), Err(IdentityError::InvalidKey));
    assert_eq!(fixed::<32>(&[0u8; 33]), Err(IdentityError::InvalidKey));
}

#[test]
fn device_public_keys_serialize_as_base64url_strings_and_round_trip() {
    let keys = DevicePublicKeys {
        signing_key: [1u8; 32],
        agreement_key: [2u8; 32],
    };

    let value = serde_json::to_value(&keys).unwrap();
    let signing_key = value.get("signing_key").unwrap().as_str().unwrap();
    let agreement_key = value.get("agreement_key").unwrap().as_str().unwrap();

    assert_eq!(signing_key.len(), 43);
    assert_eq!(agreement_key.len(), 43);

    let round_tripped: DevicePublicKeys = serde_json::from_value(value).unwrap();
    assert_eq!(round_tripped, keys);
}
