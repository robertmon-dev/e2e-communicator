use std::fmt::Write;

use orion::aead::SecretKey as AeadSecretKey;
use orion::hazardous::kdf::hkdf;
use orion::kdf::{Password, Salt, SecretKey as KdfSecretKey, derive_key};
use sea_orm::{ConnectOptions, DbErr};
use zeroize::Zeroizing;

const KDF_ITERATIONS: u32 = 3;
const KDF_MEMORY_KIB: u32 = 1 << 16;
const KDF_KEY_LEN: u32 = 32;

const MASTER_KEY_INFO: &[u8] = b"e2e-communicator/app-master-key";

fn derive_root_key(pin: &str, salt: &str) -> Result<KdfSecretKey, DbErr> {
    let constant_salt = Salt::from_slice(salt.as_bytes())
        .map_err(|_| DbErr::Custom("Invalid salt configuration".to_string()))?;
    let password = Password::from_slice(pin.as_bytes())
        .map_err(|_| DbErr::Custom("Invalid password format".to_string()))?;

    derive_key(
        &password,
        &constant_salt,
        KDF_ITERATIONS,
        KDF_MEMORY_KIB,
        KDF_KEY_LEN,
    )
    .map_err(|_| DbErr::Custom("Key derivation failed".to_string()))
}

pub fn derive_sqlcipher_key(pin: &str, salt: &str) -> Result<Zeroizing<String>, DbErr> {
    let derived_key = derive_root_key(pin, salt)?;

    let mut hex_key = Zeroizing::new(String::with_capacity((KDF_KEY_LEN * 2) as usize));
    for b in derived_key.unprotected_as_bytes() {
        let _ = write!(&mut *hex_key, "{b:02x}");
    }

    Ok(Zeroizing::new(format!("\"x'{}'\"", hex_key.as_str())))
}

pub fn derive_master_key(pin: &str, salt: &str) -> Result<AeadSecretKey, DbErr> {
    let root_key = derive_root_key(pin, salt)?;
    expand_master_key(&root_key, salt)
}

pub fn derive_master_key_pair(
    pin: &str,
    salt: &str,
) -> Result<(AeadSecretKey, AeadSecretKey), DbErr> {
    let root_key = derive_root_key(pin, salt)?;
    let first = expand_master_key(&root_key, salt)?;
    let second = expand_master_key(&root_key, salt)?;
    Ok((first, second))
}

fn expand_master_key(root_key: &KdfSecretKey, salt: &str) -> Result<AeadSecretKey, DbErr> {
    let mut derived = Zeroizing::new([0u8; 32]);
    hkdf::sha256::derive_key(
        salt.as_bytes(),
        root_key.unprotected_as_bytes(),
        Some(MASTER_KEY_INFO),
        derived.as_mut_slice(),
    )
    .map_err(|_| DbErr::Custom("Master key derivation failed".to_string()))?;

    AeadSecretKey::from_slice(derived.as_slice())
        .map_err(|_| DbErr::Custom("Master key derivation failed".to_string()))
}

pub fn connect_options(db_url: &str, pin: &str, salt: &str) -> Result<ConnectOptions, DbErr> {
    let sqlcipher_raw_key = derive_sqlcipher_key(pin, salt)?;

    let mut options = ConnectOptions::new(db_url);
    options
        .sqlx_logging(false)
        .sqlcipher_key(sqlcipher_raw_key.as_str().to_owned());

    Ok(options)
}
