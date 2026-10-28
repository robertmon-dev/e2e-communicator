mod fixtures;

use std::collections::HashSet;
use std::time::Duration;

use common::identity::{
    DevicePublicKeys, NONCE_LEN, canonical_device_bytes, canonical_device_cert_bytes,
    generate_signing_seed, random_bytes, sign, signing_public_key, verify,
};
use fixtures::TestDevice;
use server::core::initializers::settings::AuthSettings;
use server::domain::dto::errors::RegistryError;
use server::domain::services::registry::Registry;
use uuid::Uuid;

fn device_keys(seed: &[u8; 32]) -> DevicePublicKeys {
    DevicePublicKeys {
        signing_key: signing_public_key(seed),
        agreement_key: random_bytes::<32>().unwrap(),
    }
}

#[tokio::test]
async fn first_device_registers_and_is_listed() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());

    fixtures::register(&registry, &device).await;

    let devices = registry
        .lookup_devices(device.user_id)
        .await
        .unwrap()
        .devices;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].device_id, device.device_id);
    assert_eq!(devices[0].keys, device.keys);
    assert!(
        (chrono::Utc::now() - devices[0].registered_at)
            .num_seconds()
            .abs()
            < 60
    );
}

#[tokio::test]
async fn concurrent_first_registrations_yield_exactly_one_device() {
    let registry = fixtures::registry();
    let user_id = Uuid::new_v4();
    let a = TestDevice::new(user_id);
    let b = TestDevice::new(user_id);
    let a_proof = a.proof();
    let b_proof = b.proof();
    let a_user_key = a.user_signing_key();
    let a_cert = a.cert();
    let b_user_key = b.user_signing_key();
    let b_cert = b.cert();

    let (ra, rb) = tokio::join!(
        registry.register_device(
            user_id,
            a.device_id,
            &a.keys,
            &a_proof,
            &a_user_key,
            &a_cert
        ),
        registry.register_device(
            user_id,
            b.device_id,
            &b.keys,
            &b_proof,
            &b_user_key,
            &b_cert
        ),
    );

    assert!(
        ra.is_ok() != rb.is_ok(),
        "exactly one registration must win: {ra:?} / {rb:?}"
    );
    assert_eq!(
        registry
            .lookup_devices(user_id)
            .await
            .unwrap()
            .devices
            .len(),
        1
    );
}

#[tokio::test]
async fn identical_re_registration_is_idempotent() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());

    fixtures::register(&registry, &device).await;
    fixtures::register(&registry, &device).await;

    let devices = registry
        .lookup_devices(device.user_id)
        .await
        .unwrap()
        .devices;
    assert_eq!(devices.len(), 1);
}

#[tokio::test]
async fn a_different_device_cannot_claim_a_taken_user_id() {
    let registry = fixtures::registry();
    let user_id = Uuid::new_v4();
    let first = TestDevice::new(user_id);
    let second = TestDevice::new(user_id);

    fixtures::register(&registry, &first).await;
    let result = registry
        .register_device(
            user_id,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await;

    assert!(matches!(result, Err(RegistryError::UserTaken)));
}

#[tokio::test]
async fn same_device_with_different_keys_is_rejected() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;

    let mut rotated = TestDevice::new(device.user_id);
    rotated.device_id = device.device_id;
    let result = registry
        .register_device(
            device.user_id,
            device.device_id,
            &rotated.keys,
            &rotated.proof(),
            &rotated.user_signing_key(),
            &rotated.cert(),
        )
        .await;

    assert!(matches!(result, Err(RegistryError::DeviceTaken)));
}

#[tokio::test]
async fn registration_with_a_bad_proof_is_rejected_and_writes_nothing() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    let other = TestDevice::new(device.user_id);

    let result = registry
        .register_device(
            device.user_id,
            device.device_id,
            &device.keys,
            &other.proof(),
            &device.user_signing_key(),
            &device.cert(),
        )
        .await;

    assert!(matches!(result, Err(RegistryError::InvalidProof)));
    assert!(
        registry
            .lookup_devices(device.user_id)
            .await
            .unwrap()
            .devices
            .is_empty()
    );
}

#[tokio::test]
async fn lookup_of_an_unknown_user_is_empty() {
    let registry = fixtures::registry();
    assert!(
        registry
            .lookup_devices(Uuid::new_v4())
            .await
            .unwrap()
            .devices
            .is_empty()
    );
}

#[tokio::test]
async fn a_link_token_adds_a_second_device_and_is_single_use() {
    let registry = fixtures::registry();
    let first = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &first).await;

    let link = registry.create_link_token(first.user_id).await.unwrap();
    assert!(link.expires_at > chrono::Utc::now());

    let second = TestDevice::new(first.user_id);
    let linked_user = registry
        .link_device(
            &link.token,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await
        .unwrap();
    assert_eq!(linked_user, first.user_id);

    let devices = registry
        .lookup_devices(first.user_id)
        .await
        .unwrap()
        .devices;
    assert_eq!(devices.len(), 2);
    let device_ids: HashSet<Uuid> = devices.iter().map(|d| d.device_id).collect();
    assert_eq!(
        device_ids,
        HashSet::from([first.device_id, second.device_id])
    );

    let third = TestDevice::new(first.user_id);
    let reuse = registry
        .link_device(
            &link.token,
            third.device_id,
            &third.keys,
            &third.proof(),
            &third.user_signing_key(),
            &third.cert(),
        )
        .await;
    assert!(matches!(reuse, Err(RegistryError::LinkTokenInvalid)));
}

#[tokio::test]
async fn a_linked_device_can_re_register_itself_and_the_first_device_is_unaffected() {
    let registry = fixtures::registry();
    let first = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &first).await;

    let link = registry.create_link_token(first.user_id).await.unwrap();
    let second = TestDevice::new(first.user_id);
    registry
        .link_device(
            &link.token,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await
        .unwrap();

    registry
        .register_device(
            first.user_id,
            first.device_id,
            &first.keys,
            &first.proof(),
            &first.user_signing_key(),
            &first.cert(),
        )
        .await
        .unwrap();
    registry
        .register_device(
            first.user_id,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await
        .unwrap();

    let devices = registry
        .lookup_devices(first.user_id)
        .await
        .unwrap()
        .devices;
    assert_eq!(devices.len(), 2);
}

#[tokio::test]
async fn linking_a_conflicting_device_id_does_not_consume_the_token() {
    let registry = fixtures::registry();
    let first = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &first).await;
    let link = registry.create_link_token(first.user_id).await.unwrap();

    let mut rotated = TestDevice::new(first.user_id);
    rotated.device_id = first.device_id;
    let conflict = registry
        .link_device(
            &link.token,
            rotated.device_id,
            &rotated.keys,
            &rotated.proof(),
            &rotated.user_signing_key(),
            &rotated.cert(),
        )
        .await;
    assert!(matches!(conflict, Err(RegistryError::DeviceTaken)));

    let second = TestDevice::new(first.user_id);
    let linked_user = registry
        .link_device(
            &link.token,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await
        .unwrap();
    assert_eq!(linked_user, first.user_id);

    let devices = registry
        .lookup_devices(first.user_id)
        .await
        .unwrap()
        .devices;
    assert_eq!(devices.len(), 2);
}

#[tokio::test]
async fn re_linking_the_same_device_with_identical_keys_is_idempotent_and_consumes_each_token_once()
{
    let registry = fixtures::registry();
    let first = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &first).await;

    let link1 = registry.create_link_token(first.user_id).await.unwrap();
    let second = TestDevice::new(first.user_id);
    registry
        .link_device(
            &link1.token,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await
        .unwrap();

    let link2 = registry.create_link_token(first.user_id).await.unwrap();
    let linked_user = registry
        .link_device(
            &link2.token,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await
        .unwrap();
    assert_eq!(linked_user, first.user_id);

    let reuse = registry
        .link_device(
            &link2.token,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await;
    assert!(matches!(reuse, Err(RegistryError::LinkTokenInvalid)));

    let devices = registry
        .lookup_devices(first.user_id)
        .await
        .unwrap()
        .devices;
    assert_eq!(devices.len(), 2);
}

#[tokio::test]
async fn an_unknown_link_token_is_rejected() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());

    let result = registry
        .link_device(
            "nope",
            device.device_id,
            &device.keys,
            &device.proof(),
            &device.user_signing_key(),
            &device.cert(),
        )
        .await;

    assert!(matches!(result, Err(RegistryError::LinkTokenInvalid)));
}

#[tokio::test]
async fn a_link_token_expires() {
    let registry = fixtures::registry_with(AuthSettings {
        link_token_ttl_secs: 1,
        ..AuthSettings::default()
    });
    let first = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &first).await;
    let link = registry.create_link_token(first.user_id).await.unwrap();

    tokio::time::sleep(Duration::from_millis(1_200)).await;

    let second = TestDevice::new(first.user_id);
    let result = registry
        .link_device(
            &link.token,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await;
    assert!(matches!(result, Err(RegistryError::LinkTokenInvalid)));
}

#[tokio::test]
async fn a_bad_proof_does_not_consume_the_link_token() {
    let registry = fixtures::registry();
    let first = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &first).await;
    let link = registry.create_link_token(first.user_id).await.unwrap();

    let second = TestDevice::new(first.user_id);
    let impostor = TestDevice::new(first.user_id);
    let bad = registry
        .link_device(
            &link.token,
            second.device_id,
            &second.keys,
            &impostor.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await;
    assert!(matches!(bad, Err(RegistryError::InvalidProof)));

    let good = registry
        .link_device(
            &link.token,
            second.device_id,
            &second.keys,
            &second.proof(),
            &second.user_signing_key(),
            &second.cert(),
        )
        .await;
    assert!(good.is_ok());
}

#[tokio::test]
async fn revoking_a_device_removes_it_and_its_sessions() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;
    assert!(registry.session(&session.token).await.unwrap().is_some());

    registry
        .revoke_device(device.user_id, device.device_id)
        .await
        .unwrap();

    assert!(
        registry
            .lookup_devices(device.user_id)
            .await
            .unwrap()
            .devices
            .is_empty()
    );
    assert!(registry.session(&session.token).await.unwrap().is_none());
}

#[tokio::test]
async fn challenge_response_creates_a_session() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;

    let session = fixtures::login(&registry, &device).await;
    assert_eq!(session.user_id, device.user_id);
    assert_eq!(session.device_id, device.device_id);
    assert!(session.expires_at > chrono::Utc::now());

    let resolved = registry.session(&session.token).await.unwrap().unwrap();
    assert_eq!(resolved, session);
}

#[tokio::test]
async fn a_wrong_signature_is_rejected() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    let impostor = TestDevice::new(device.user_id);
    fixtures::register(&registry, &device).await;

    let nonce = registry
        .issue_challenge(device.user_id, device.device_id)
        .await
        .unwrap();
    let result = registry
        .authenticate(
            device.user_id,
            device.device_id,
            &impostor.sign_challenge(&nonce),
        )
        .await;

    assert!(matches!(result, Err(RegistryError::InvalidSignature)));
}

#[tokio::test]
async fn a_nonce_cannot_be_used_twice() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;

    let nonce = registry
        .issue_challenge(device.user_id, device.device_id)
        .await
        .unwrap();
    let signature = device.sign_challenge(&nonce);
    let _session = registry
        .authenticate(device.user_id, device.device_id, &signature)
        .await
        .unwrap();

    let replay = registry
        .authenticate(device.user_id, device.device_id, &signature)
        .await;
    assert!(matches!(replay, Err(RegistryError::ChallengeMissing)));
}

#[tokio::test]
async fn an_expired_nonce_is_rejected() {
    let registry = fixtures::registry_with(AuthSettings {
        challenge_ttl_secs: 1,
        ..AuthSettings::default()
    });
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;

    let nonce = registry
        .issue_challenge(device.user_id, device.device_id)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1_200)).await;

    let result = registry
        .authenticate(
            device.user_id,
            device.device_id,
            &device.sign_challenge(&nonce),
        )
        .await;
    assert!(matches!(result, Err(RegistryError::ChallengeMissing)));
}

#[tokio::test]
async fn unknown_devices_get_no_challenge() {
    let registry = fixtures::registry();
    let result = registry
        .issue_challenge(Uuid::new_v4(), Uuid::new_v4())
        .await;
    assert!(matches!(result, Err(RegistryError::UnknownDevice)));
}

#[tokio::test]
async fn the_session_index_set_has_a_bounded_ttl() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let _session = fixtures::login(&registry, &device).await;

    let mut conn = fixtures::store().connection().await.unwrap();
    let ttl: i64 = redis::cmd("TTL")
        .arg(format!("sessions:{}:{}", device.user_id, device.device_id))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(
        ttl > 0,
        "expected a positive TTL on the session index, got {ttl}"
    );
}

#[tokio::test]
async fn revoking_an_unknown_device_is_a_no_op() {
    let registry = fixtures::registry();
    assert!(
        registry
            .revoke_device(Uuid::new_v4(), Uuid::new_v4())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn authenticating_without_a_prior_challenge_is_rejected() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;

    let result = registry
        .authenticate(
            device.user_id,
            device.device_id,
            &device.sign_challenge(&[0u8; NONCE_LEN]),
        )
        .await;
    assert!(matches!(result, Err(RegistryError::ChallengeMissing)));
}

#[tokio::test]
async fn a_challenge_cannot_be_redeemed_under_a_different_device_id_and_is_left_consumable() {
    let registry = fixtures::registry();
    let device_a = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device_a).await;

    let link = registry.create_link_token(device_a.user_id).await.unwrap();
    let device_b = TestDevice::new(device_a.user_id);
    registry
        .link_device(
            &link.token,
            device_b.device_id,
            &device_b.keys,
            &device_b.proof(),
            &device_b.user_signing_key(),
            &device_b.cert(),
        )
        .await
        .unwrap();

    let nonce = registry
        .issue_challenge(device_a.user_id, device_a.device_id)
        .await
        .unwrap();

    let wrong_device = registry
        .authenticate(
            device_a.user_id,
            device_b.device_id,
            &device_b.sign_challenge(&nonce),
        )
        .await;
    assert!(matches!(wrong_device, Err(RegistryError::ChallengeMissing)));

    let session = registry
        .authenticate(
            device_a.user_id,
            device_a.device_id,
            &device_a.sign_challenge(&nonce),
        )
        .await
        .unwrap();
    assert_eq!(session.device_id, device_a.device_id);
}

#[tokio::test]
async fn an_unknown_session_token_resolves_to_none() {
    let registry = fixtures::registry();
    assert!(registry.session("nope").await.unwrap().is_none());
}

#[tokio::test]
async fn sessions_expire() {
    let registry = fixtures::registry_with(AuthSettings {
        session_ttl_secs: 1,
        ..AuthSettings::default()
    });
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;
    let session = fixtures::login(&registry, &device).await;

    tokio::time::sleep(Duration::from_millis(1_200)).await;

    assert!(registry.session(&session.token).await.unwrap().is_none());
}

#[tokio::test]
async fn a_challenge_issued_before_revocation_cannot_be_redeemed_afterwards() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());
    fixtures::register(&registry, &device).await;

    let nonce = registry
        .issue_challenge(device.user_id, device.device_id)
        .await
        .unwrap();

    registry
        .revoke_device(device.user_id, device.device_id)
        .await
        .unwrap();

    let result = registry
        .authenticate(
            device.user_id,
            device.device_id,
            &device.sign_challenge(&nonce),
        )
        .await;

    assert!(matches!(result, Err(RegistryError::UnknownDevice)));
}

async fn register_certified_device(
    registry: &Registry,
    user_id: Uuid,
    user_seed: &[u8; 32],
) -> Result<Uuid, RegistryError> {
    let device_id = Uuid::new_v4();
    let device_seed = generate_signing_seed().unwrap();
    let keys = device_keys(&device_seed);
    let self_proof = sign(
        &device_seed,
        &canonical_device_bytes(user_id, device_id, &keys),
    );
    let cert = sign(
        user_seed,
        &canonical_device_cert_bytes(user_id, device_id, &keys),
    );

    registry
        .register_device(
            user_id,
            device_id,
            &keys,
            &self_proof,
            &signing_public_key(user_seed),
            &cert,
        )
        .await
        .map(|_| device_id)
}

#[tokio::test]
async fn register_device_rejects_cert_signed_by_another_key() {
    let registry = fixtures::registry();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let device_seed = generate_signing_seed().unwrap();
    let keys = device_keys(&device_seed);
    let self_proof = sign(
        &device_seed,
        &canonical_device_bytes(user_id, device_id, &keys),
    );

    let attacker_seed = generate_signing_seed().unwrap();
    let honest_seed = generate_signing_seed().unwrap();
    let cert = sign(
        &attacker_seed,
        &canonical_device_cert_bytes(user_id, device_id, &keys),
    );

    let result = registry
        .register_device(
            user_id,
            device_id,
            &keys,
            &self_proof,
            &signing_public_key(&honest_seed),
            &cert,
        )
        .await;

    assert!(matches!(result, Err(RegistryError::InvalidProof)));
}

#[tokio::test]
async fn second_device_with_a_different_user_key_is_rejected() {
    let registry = fixtures::registry();
    let user_id = Uuid::new_v4();
    let user_seed = generate_signing_seed().unwrap();
    register_certified_device(&registry, user_id, &user_seed)
        .await
        .unwrap();

    let other_user_seed = generate_signing_seed().unwrap();
    let result = register_certified_device(&registry, user_id, &other_user_seed).await;

    assert!(matches!(result, Err(RegistryError::UserTaken)));
}

#[tokio::test]
async fn lookup_devices_returns_user_key_and_cert() {
    let registry = fixtures::registry();
    let user_id = Uuid::new_v4();
    let user_seed = generate_signing_seed().unwrap();
    let device_id = register_certified_device(&registry, user_id, &user_seed)
        .await
        .unwrap();

    let directory = registry.lookup_devices(user_id).await.unwrap();

    assert_eq!(directory.user_signing_key, signing_public_key(&user_seed));
    assert_eq!(directory.devices.len(), 1);
    assert_eq!(directory.devices[0].device_id, device_id);

    let message = canonical_device_cert_bytes(user_id, device_id, &directory.devices[0].keys);
    assert!(
        verify(
            &directory.user_signing_key,
            &message,
            &directory.devices[0].cert
        )
        .is_ok(),
        "zwrocony certyfikat musi weryfikowac sie kluczem usera"
    );
}

#[tokio::test]
async fn re_registering_the_same_device_stays_idempotent() {
    let registry = fixtures::registry();
    let user_id = Uuid::new_v4();
    let user_seed = generate_signing_seed().unwrap();
    let device_id = Uuid::new_v4();
    let device_seed = generate_signing_seed().unwrap();
    let keys = device_keys(&device_seed);
    let self_proof = sign(
        &device_seed,
        &canonical_device_bytes(user_id, device_id, &keys),
    );
    let cert = sign(
        &user_seed,
        &canonical_device_cert_bytes(user_id, device_id, &keys),
    );
    let user_key = signing_public_key(&user_seed);

    registry
        .register_device(user_id, device_id, &keys, &self_proof, &user_key, &cert)
        .await
        .unwrap();

    registry
        .register_device(user_id, device_id, &keys, &self_proof, &user_key, &cert)
        .await
        .expect("powtorna rejestracja tego samego urzadzenia musi przejsc");

    assert_eq!(
        registry
            .lookup_devices(user_id)
            .await
            .unwrap()
            .devices
            .len(),
        1
    );
}

#[tokio::test]
async fn lookup_devices_for_a_user_with_devices_but_no_pinned_key_is_corrupt() {
    let registry = fixtures::registry();
    let user_id = Uuid::new_v4();
    let device = TestDevice::new(user_id);
    fixtures::register(&registry, &device).await;

    let mut conn = fixtures::store().connection().await.unwrap();
    let _: () = redis::cmd("DEL")
        .arg(format!("identity:user:{user_id}:key"))
        .query_async(&mut conn)
        .await
        .unwrap();

    let result = registry.lookup_devices(user_id).await;

    assert!(matches!(result, Err(RegistryError::Corrupt(_))));
}
