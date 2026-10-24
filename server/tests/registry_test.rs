mod fixtures;

use fixtures::TestDevice;
use server::core::initializers::settings::AuthSettings;
use server::domain::dto::errors::RegistryError;
use std::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn first_device_registers_and_is_listed() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());

    fixtures::register(&registry, &device).await;

    let devices = registry.lookup_devices(device.user_id).await.unwrap();
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

    let (ra, rb) = tokio::join!(
        registry.register_device(user_id, a.device_id, &a.keys, &a_proof),
        registry.register_device(user_id, b.device_id, &b.keys, &b_proof),
    );

    assert!(
        ra.is_ok() != rb.is_ok(),
        "exactly one registration must win: {ra:?} / {rb:?}"
    );
    assert_eq!(registry.lookup_devices(user_id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn identical_re_registration_is_idempotent() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());

    fixtures::register(&registry, &device).await;
    fixtures::register(&registry, &device).await;

    let devices = registry.lookup_devices(device.user_id).await.unwrap();
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
        .register_device(user_id, second.device_id, &second.keys, &second.proof())
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
        )
        .await;

    assert!(matches!(result, Err(RegistryError::UserTaken)));
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
        )
        .await;

    assert!(matches!(result, Err(RegistryError::InvalidProof)));
    assert!(
        registry
            .lookup_devices(device.user_id)
            .await
            .unwrap()
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
        .link_device(&link.token, second.device_id, &second.keys, &second.proof())
        .await
        .unwrap();
    assert_eq!(linked_user, first.user_id);

    let devices = registry.lookup_devices(first.user_id).await.unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].device_id, first.device_id);
    assert_eq!(devices[1].device_id, second.device_id);

    let third = TestDevice::new(first.user_id);
    let reuse = registry
        .link_device(&link.token, third.device_id, &third.keys, &third.proof())
        .await;
    assert!(matches!(reuse, Err(RegistryError::LinkTokenInvalid)));
}

#[tokio::test]
async fn an_unknown_link_token_is_rejected() {
    let registry = fixtures::registry();
    let device = TestDevice::new(Uuid::new_v4());

    let result = registry
        .link_device("nope", device.device_id, &device.keys, &device.proof())
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
        .link_device(&link.token, second.device_id, &second.keys, &second.proof())
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
        )
        .await;
    assert!(matches!(bad, Err(RegistryError::InvalidProof)));

    let good = registry
        .link_device(&link.token, second.device_id, &second.keys, &second.proof())
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
    registry
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
