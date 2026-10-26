use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;
pub const NONCE_LEN: usize = 32;
pub const FINGERPRINT_LEN: usize = 16;

const DEVICE_DOMAIN: &[u8] = b"e2e-communicator/device/v1";
const CHALLENGE_DOMAIN: &[u8] = b"e2e-communicator/challenge/v1";
const DEVICE_CERT_DOMAIN: &[u8] = b"e2e-communicator/device-cert/v1";
const CHAT_KEY_DOMAIN: &[u8] = b"e2e-communicator/chat-key/v1";
const USER_FINGERPRINT_DOMAIN: &[u8] = b"e2e-communicator/user-fp/v1";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdentityError {
    #[error("key material has invalid length or encoding")]
    InvalidKey,

    #[error("signature verification failed")]
    InvalidSignature,

    #[error("random number generation failed")]
    RandomFailed,

    #[error("token is not valid base64url")]
    InvalidToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevicePublicKeys {
    #[serde(with = "b64")]
    pub signing_key: [u8; KEY_LEN],
    #[serde(with = "b64")]
    pub agreement_key: [u8; KEY_LEN],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactCode {
    pub user_id: Uuid,
    pub fingerprint: [u8; FINGERPRINT_LEN],
}

mod b64 {
    use serde::{Deserialize, Deserializer, Serializer};

    use super::{KEY_LEN, decode_token_array, encode_token};

    pub fn serialize<S>(bytes: &[u8; KEY_LEN], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&encode_token(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; KEY_LEN], D::Error>
    where
        D: Deserializer<'de>,
    {
        let token = String::deserialize(deserializer)?;
        decode_token_array::<KEY_LEN>(&token).map_err(serde::de::Error::custom)
    }
}

impl DevicePublicKeys {
    pub fn from_slices(signing_key: &[u8], agreement_key: &[u8]) -> Result<Self, IdentityError> {
        Ok(Self {
            signing_key: fixed(signing_key)?,
            agreement_key: fixed(agreement_key)?,
        })
    }
}

pub fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], IdentityError> {
    bytes.try_into().map_err(|_| IdentityError::InvalidKey)
}

pub fn random_bytes<const N: usize>() -> Result<[u8; N], IdentityError> {
    let mut buf = [0u8; N];
    orion::util::secure_rand_bytes(&mut buf).map_err(|_| IdentityError::RandomFailed)?;
    Ok(buf)
}

pub fn generate_signing_seed() -> Result<Zeroizing<[u8; KEY_LEN]>, IdentityError> {
    random_bytes::<KEY_LEN>().map(Zeroizing::new)
}

pub fn signing_public_key(seed: &[u8; KEY_LEN]) -> [u8; KEY_LEN] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

pub fn canonical_device_bytes(user_id: Uuid, device_id: Uuid, keys: &DevicePublicKeys) -> Vec<u8> {
    let mut out = Vec::with_capacity(DEVICE_DOMAIN.len() + 16 + 16 + KEY_LEN * 2);
    out.extend_from_slice(DEVICE_DOMAIN);
    out.extend_from_slice(user_id.as_bytes());
    out.extend_from_slice(device_id.as_bytes());
    out.extend_from_slice(&keys.signing_key);
    out.extend_from_slice(&keys.agreement_key);
    out
}

pub fn canonical_device_cert_bytes(
    user_id: Uuid,
    device_id: Uuid,
    keys: &DevicePublicKeys,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(DEVICE_CERT_DOMAIN.len() + 16 + 16 + KEY_LEN * 2);
    out.extend_from_slice(DEVICE_CERT_DOMAIN);
    out.extend_from_slice(user_id.as_bytes());
    out.extend_from_slice(device_id.as_bytes());
    out.extend_from_slice(&keys.signing_key);
    out.extend_from_slice(&keys.agreement_key);
    out
}

pub fn canonical_chat_key_bytes(
    chat_id: Uuid,
    user_id: Uuid,
    device_id: Uuid,
    chat_public_key: &[u8; KEY_LEN],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(CHAT_KEY_DOMAIN.len() + 16 * 3 + KEY_LEN);
    out.extend_from_slice(CHAT_KEY_DOMAIN);
    out.extend_from_slice(chat_id.as_bytes());
    out.extend_from_slice(user_id.as_bytes());
    out.extend_from_slice(device_id.as_bytes());
    out.extend_from_slice(chat_public_key);
    out
}

pub fn canonical_challenge_bytes(
    user_id: Uuid,
    device_id: Uuid,
    nonce: &[u8; NONCE_LEN],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(CHALLENGE_DOMAIN.len() + 16 + 16 + NONCE_LEN);
    out.extend_from_slice(CHALLENGE_DOMAIN);
    out.extend_from_slice(user_id.as_bytes());
    out.extend_from_slice(device_id.as_bytes());
    out.extend_from_slice(nonce);
    out
}

pub fn sign(seed: &[u8; KEY_LEN], message: &[u8]) -> [u8; SIGNATURE_LEN] {
    SigningKey::from_bytes(seed).sign(message).to_bytes()
}

pub fn verify(
    public_key: &[u8; KEY_LEN],
    message: &[u8],
    signature: &[u8; SIGNATURE_LEN],
) -> Result<(), IdentityError> {
    let verifying_key =
        VerifyingKey::from_bytes(public_key).map_err(|_| IdentityError::InvalidKey)?;
    let signature = Signature::from_bytes(signature);
    verifying_key
        .verify_strict(message, &signature)
        .map_err(|_| IdentityError::InvalidSignature)
}

pub fn encode_token(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn decode_token(token: &str) -> Result<Vec<u8>, IdentityError> {
    URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| IdentityError::InvalidToken)
}

pub fn decode_token_array<const N: usize>(token: &str) -> Result<[u8; N], IdentityError> {
    let bytes = decode_token(token)?;
    fixed(&bytes).map_err(|_| IdentityError::InvalidToken)
}

pub fn user_fingerprint(user_signing_key: &[u8; KEY_LEN]) -> [u8; FINGERPRINT_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(USER_FINGERPRINT_DOMAIN);
    hasher.update(user_signing_key);
    let digest = hasher.finalize();

    let mut out = [0u8; FINGERPRINT_LEN];
    out.copy_from_slice(&digest[..FINGERPRINT_LEN]);
    out
}

pub fn safety_number(key_a: &[u8; KEY_LEN], key_b: &[u8; KEY_LEN]) -> String {
    let (first, second) = if key_a <= key_b {
        (key_a, key_b)
    } else {
        (key_b, key_a)
    };

    let mut hasher = Sha256::new();
    hasher.update(first);
    hasher.update(second);
    let digest = hasher.finalize();

    digest[..12]
        .chunks(2)
        .map(|pair| format!("{:05}", u16::from_be_bytes([pair[0], pair[1]])))
        .collect::<Vec<String>>()
        .join(" ")
}

pub fn encode_contact_code(user_id: Uuid, fingerprint: &[u8; FINGERPRINT_LEN]) -> String {
    let mut raw = Vec::with_capacity(16 + FINGERPRINT_LEN);
    raw.extend_from_slice(user_id.as_bytes());
    raw.extend_from_slice(fingerprint);
    encode_token(&raw)
}

pub fn decode_contact_code(code: &str) -> Result<ContactCode, IdentityError> {
    let raw = decode_token(code.trim())?;
    if raw.len() != 16 + FINGERPRINT_LEN {
        return Err(IdentityError::InvalidToken);
    }

    let user_bytes: [u8; 16] = fixed(&raw[..16]).map_err(|_| IdentityError::InvalidToken)?;
    let fingerprint: [u8; FINGERPRINT_LEN] =
        fixed(&raw[16..]).map_err(|_| IdentityError::InvalidToken)?;

    Ok(ContactCode {
        user_id: Uuid::from_bytes(user_bytes),
        fingerprint,
    })
}
