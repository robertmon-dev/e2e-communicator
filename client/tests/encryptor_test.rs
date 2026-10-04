use orion::aead::SecretKey;

use client::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};

#[test]
fn decrypt_with_derived_key_reverses_encrypt_with_derived_key() {
    let key = SecretKey::default();
    let plaintext = b"hello from the other side";

    let ciphertext = Encryptor::encrypt_with_derived_key(&key, plaintext).unwrap();
    let decrypted = Encryptor::decrypt_with_derived_key(&key, &ciphertext).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn decrypt_with_derived_key_rejects_wrong_key() {
    let key = SecretKey::default();
    let wrong_key = SecretKey::default();
    let ciphertext = Encryptor::encrypt_with_derived_key(&key, b"secret").unwrap();

    let result = Encryptor::decrypt_with_derived_key(&wrong_key, &ciphertext);

    assert!(result.is_err());
}
