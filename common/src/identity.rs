use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;
pub const NONCE_LEN: usize = 32;

const DEVICE_DOMAIN: &[u8] = b"e2e-communicator/device/v1";
const CHALLENGE_DOMAIN: &[u8] = b"e2e-communicator/challenge/v1";

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
    pub signing_key: [u8; KEY_LEN],
    pub agreement_key: [u8; KEY_LEN],
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
