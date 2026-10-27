mod fixtures;

use std::sync::Arc;

use client::adapters::out::persistance::repositories::peer_device::PeerDeviceRepository;
use client::domain::dto::errors::PeerDirectoryError;
use client::domain::services::peer_directory::PeerDirectory;
use common::identity::{
    FINGERPRINT_LEN, KEY_LEN, random_bytes, signing_public_key, user_fingerprint,
};
use fixtures::{FakeIdentityTransport, registered_identity, seed_user, setup_db};
use uuid::Uuid;

async fn harness(
    db: &Arc<sea_orm::DatabaseConnection>,
) -> (
    PeerDirectory,
    Arc<FakeIdentityTransport>,
    Arc<PeerDeviceRepository>,
) {
    let local_user_id = Uuid::new_v4();
    let identity = registered_identity(local_user_id);
    let transport = FakeIdentityTransport::new();
    let peer_devices = Arc::new(PeerDeviceRepository::new(db.clone()));

    let directory = PeerDirectory::new(transport.clone(), peer_devices.clone(), identity);

    (directory, transport, peer_devices)
}

#[tokio::test]
async fn resolve_pins_the_identity_when_the_fingerprint_matches() {
    let db = setup_db().await;
    let (directory, transport, peer_devices) = harness(&db).await;
    let peer_id = Uuid::new_v4();
    seed_user(&db, peer_id).await;

    let user_seed = random_bytes::<KEY_LEN>().unwrap();
    let user_key = signing_public_key(&user_seed);
    let device_id = transport.seed_certified_device(peer_id, &user_seed);

    let resolved = directory
        .resolve(peer_id, Some(user_fingerprint(&user_key)))
        .await
        .unwrap();

    assert_eq!(resolved.user_id, peer_id);
    assert_eq!(resolved.user_signing_key, user_key);
    assert_eq!(resolved.devices.len(), 1);
    assert_eq!(resolved.devices[0].device_id, device_id);

    assert_eq!(
        peer_devices.pinned_identity(peer_id).await.unwrap(),
        Some(user_key.to_vec())
    );
}

#[tokio::test]
async fn resolve_rejects_a_fingerprint_that_does_not_match_and_pins_nothing() {
    let db = setup_db().await;
    let (directory, transport, peer_devices) = harness(&db).await;
    let peer_id = Uuid::new_v4();
    seed_user(&db, peer_id).await;

    let user_seed = random_bytes::<KEY_LEN>().unwrap();
    transport.seed_certified_device(peer_id, &user_seed);

    let wrong_fingerprint = [0u8; FINGERPRINT_LEN];
    let result = directory.resolve(peer_id, Some(wrong_fingerprint)).await;

    assert!(matches!(
        result,
        Err(PeerDirectoryError::IdentityMismatch(id)) if id == peer_id
    ));
    assert_eq!(peer_devices.pinned_identity(peer_id).await.unwrap(), None);
}

#[tokio::test]
async fn resolve_skips_devices_with_a_cert_from_an_unrelated_key() {
    let db = setup_db().await;
    let (directory, transport, peer_devices) = harness(&db).await;
    let peer_id = Uuid::new_v4();
    seed_user(&db, peer_id).await;

    let user_seed = random_bytes::<KEY_LEN>().unwrap();
    let stranger_seed = random_bytes::<KEY_LEN>().unwrap();
    transport.seed_device_with_cert_from(peer_id, &user_seed, &stranger_seed);

    let result = directory.resolve(peer_id, None).await;

    assert!(matches!(
        result,
        Err(PeerDirectoryError::NoVerifiedDevices(id)) if id == peer_id
    ));
    assert_eq!(peer_devices.pinned_identity(peer_id).await.unwrap(), None);
}

#[tokio::test]
async fn resolve_rejects_an_identity_that_changed_since_it_was_pinned() {
    let db = setup_db().await;
    let (directory, transport, peer_devices) = harness(&db).await;
    let peer_id = Uuid::new_v4();
    seed_user(&db, peer_id).await;

    let original_seed = random_bytes::<KEY_LEN>().unwrap();
    peer_devices
        .pin_identity(peer_id, signing_public_key(&original_seed).to_vec())
        .await
        .unwrap();

    let new_seed = random_bytes::<KEY_LEN>().unwrap();
    transport.seed_certified_device(peer_id, &new_seed);

    let result = directory.resolve(peer_id, None).await;

    assert!(matches!(
        result,
        Err(PeerDirectoryError::IdentityChanged(id)) if id == peer_id
    ));
}

#[tokio::test]
async fn cached_reads_previously_pinned_devices_without_touching_the_transport() {
    let db = setup_db().await;
    let (directory, transport, _peer_devices) = harness(&db).await;
    let peer_id = Uuid::new_v4();
    seed_user(&db, peer_id).await;

    let user_seed = random_bytes::<KEY_LEN>().unwrap();
    let device_id = transport.seed_certified_device(peer_id, &user_seed);

    directory.resolve(peer_id, None).await.unwrap();
    let lookups_after_resolve = transport.count("lookup_devices");

    let cached = directory.cached(peer_id).await.unwrap();

    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].device_id, device_id);
    assert_eq!(transport.count("lookup_devices"), lookups_after_resolve);
}

#[tokio::test]
async fn cached_without_any_pinned_device_is_an_error() {
    let db = setup_db().await;
    let peer_id = Uuid::new_v4();
    seed_user(&db, peer_id).await;

    let directory = PeerDirectory::new(
        FakeIdentityTransport::new(),
        Arc::new(PeerDeviceRepository::new(db.clone())),
        registered_identity(Uuid::new_v4()),
    );

    let result = directory.cached(peer_id).await;

    assert!(
        matches!(result, Err(PeerDirectoryError::NoVerifiedDevices(id)) if id == peer_id),
        "pusty cache nie moze udawac udanej wysylki do zera urzadzen"
    );
}
