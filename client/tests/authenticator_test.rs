mod fixtures;

use std::sync::Arc;
use std::time::Duration;

use client::domain::dto::errors::{AuthError, TransportError};
use client::domain::state::connectivity::ConnectivityStatus;
use client::domain::state::identity::IdentityStatus;
use fixtures::{
    FakeIdentityTransport, build_authenticator, registered_harness, setup_db, wait_until,
};
use uuid::Uuid;

#[tokio::test]
async fn register_new_creates_an_identity_and_flips_the_state() {
    let db = setup_db().await;
    let harness = build_authenticator(
        &db,
        FakeIdentityTransport::new(),
        IdentityStatus::Unregistered,
        ConnectivityStatus::Online,
    )
    .await;

    let identity = harness.authenticator.register_new().await.unwrap();

    assert_eq!(harness.identity.registered(), Some(identity.clone()));
    assert_eq!(
        harness.identity_repository.get().await.unwrap(),
        Some(identity)
    );
}

#[tokio::test]
async fn register_new_is_rejected_when_already_registered() {
    let db = setup_db().await;
    let (harness, _) = registered_harness(&db, FakeIdentityTransport::new()).await;

    assert!(matches!(
        harness.authenticator.register_new().await,
        Err(AuthError::WrongState)
    ));
}

#[tokio::test]
async fn authenticate_once_registers_challenges_and_sets_the_session() {
    let db = setup_db().await;
    let (harness, identity) = registered_harness(&db, FakeIdentityTransport::new()).await;

    let token = harness
        .authenticator
        .authenticate_once(&identity)
        .await
        .unwrap();

    assert_eq!(harness.session.get(), Some(token));
    assert_eq!(
        harness.transport.calls(),
        vec!["register_device", "challenge", "authenticate"]
    );
}

#[tokio::test]
async fn authenticate_once_maps_already_exists_to_user_taken() {
    let db = setup_db().await;
    let transport = FakeIdentityTransport::new();
    let (harness, identity) = registered_harness(&db, transport.clone()).await;
    transport
        .taken_users
        .lock()
        .unwrap()
        .insert(identity.user_id);

    let result = harness.authenticator.authenticate_once(&identity).await;

    assert!(matches!(result, Err(AuthError::UserTaken)));
    assert!(harness.session.get().is_none());
}

#[tokio::test]
async fn run_logs_in_once_online_and_registered() {
    let db = setup_db().await;
    let transport = FakeIdentityTransport::new();
    let harness = build_authenticator(
        &db,
        transport.clone(),
        IdentityStatus::Unregistered,
        ConnectivityStatus::Offline,
    )
    .await;
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let runner = {
        let authenticator = harness.authenticator.clone();
        tokio::spawn(async move { authenticator.run(rx).await })
    };

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(harness.session.get().is_none());
    assert_eq!(transport.count("authenticate"), 0);

    harness.authenticator.register_new().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(harness.session.get().is_none(), "still offline");

    harness.connectivity.set_online();
    wait_until("session", || harness.session.get().is_some()).await;

    runner.abort();
}

#[tokio::test]
async fn run_relogs_after_the_session_is_cleared() {
    let db = setup_db().await;
    let transport = FakeIdentityTransport::new();
    let (harness, _) = registered_harness(&db, transport.clone()).await;
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let runner = {
        let authenticator = harness.authenticator.clone();
        tokio::spawn(async move { authenticator.run(rx).await })
    };
    wait_until("first session", || harness.session.get().is_some()).await;
    let first = harness.session.get().unwrap();

    harness.session.clear();
    wait_until("second session", || {
        harness
            .session
            .get()
            .is_some_and(|t| t.token != first.token)
    })
    .await;

    runner.abort();
}

#[tokio::test]
async fn run_refreshes_the_session_before_it_expires() {
    let db = setup_db().await;
    let transport = Arc::new(FakeIdentityTransport::with_session_ttl(
        Duration::from_millis(400),
    ));
    let (mut harness, _) = registered_harness(&db, transport.clone()).await;
    harness
        .reconfigure(|authenticator| authenticator.with_refresh_margin(Duration::from_millis(300)));
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let runner = {
        let authenticator = harness.authenticator.clone();
        tokio::spawn(async move { authenticator.run(rx).await })
    };

    wait_until("first login", || transport.count("authenticate") >= 1).await;
    wait_until("refresh", || transport.count("authenticate") >= 2).await;
    assert!(
        harness.session.get().is_some(),
        "session is replaced, never cleared"
    );

    runner.abort();
}

#[tokio::test]
async fn run_moves_to_conflict_after_repeated_unauthenticated() {
    let db = setup_db().await;
    let transport = FakeIdentityTransport::new();
    let (harness, _) = registered_harness(&db, transport.clone()).await;
    {
        let mut failures = transport.authenticate_failures.lock().unwrap();
        for _ in 0..3 {
            failures.push_back(TransportError::Unauthenticated("revoked".to_string()));
        }
    }
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let runner = {
        let authenticator = harness.authenticator.clone();
        tokio::spawn(async move { authenticator.run(rx).await })
    };

    wait_until("conflict", || {
        matches!(harness.identity.current(), IdentityStatus::Conflict(_))
    })
    .await;
    assert!(harness.session.get().is_none());

    runner.abort();
}

#[tokio::test]
async fn run_moves_to_conflict_when_the_user_id_is_taken() {
    let db = setup_db().await;
    let transport = FakeIdentityTransport::new();
    let (harness, identity) = registered_harness(&db, transport.clone()).await;
    transport
        .taken_users
        .lock()
        .unwrap()
        .insert(identity.user_id);
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let runner = {
        let authenticator = harness.authenticator.clone();
        tokio::spawn(async move { authenticator.run(rx).await })
    };

    wait_until("conflict", || {
        matches!(harness.identity.current(), IdentityStatus::Conflict(_))
    })
    .await;

    runner.abort();
}

#[tokio::test]
async fn link_creates_an_identity_with_the_given_user_id() {
    let db = setup_db().await;
    let transport = FakeIdentityTransport::new();
    let harness = build_authenticator(
        &db,
        transport.clone(),
        IdentityStatus::Unregistered,
        ConnectivityStatus::Online,
    )
    .await;
    let existing_user = Uuid::new_v4();
    transport
        .link_tokens
        .lock()
        .unwrap()
        .insert("tok".to_string(), existing_user);

    let identity = harness
        .authenticator
        .link(existing_user, "tok")
        .await
        .unwrap();

    assert_eq!(identity.user_id, existing_user);
    assert_eq!(harness.identity.registered(), Some(identity));
}

#[tokio::test]
async fn a_rejected_link_persists_nothing() {
    let db = setup_db().await;
    let harness = build_authenticator(
        &db,
        FakeIdentityTransport::new(),
        IdentityStatus::Unregistered,
        ConnectivityStatus::Online,
    )
    .await;

    let result = harness.authenticator.link(Uuid::new_v4(), "unknown").await;

    assert!(matches!(
        result,
        Err(AuthError::Transport(TransportError::NotFound(_)))
    ));
    assert!(harness.identity_repository.get().await.unwrap().is_none());
    assert_eq!(harness.identity.current(), IdentityStatus::Unregistered);
}

#[tokio::test]
async fn link_token_and_device_listing_require_a_session() {
    let db = setup_db().await;
    let (harness, _) = registered_harness(&db, FakeIdentityTransport::new()).await;

    assert!(matches!(
        harness.authenticator.create_link_token().await,
        Err(AuthError::NoSession)
    ));
    assert!(matches!(
        harness.authenticator.list_devices().await,
        Err(AuthError::NoSession)
    ));
}

#[tokio::test]
async fn revoking_own_device_logs_out_and_forgets_the_identity() {
    let db = setup_db().await;
    let transport = FakeIdentityTransport::new();
    let (harness, identity) = registered_harness(&db, transport.clone()).await;
    harness
        .authenticator
        .authenticate_once(&identity)
        .await
        .unwrap();

    harness
        .authenticator
        .revoke_device(identity.device_id)
        .await
        .unwrap();

    assert_eq!(
        transport.revoked.lock().unwrap().as_slice(),
        &[identity.device_id]
    );
    assert!(harness.session.get().is_none());
    assert!(harness.identity_repository.get().await.unwrap().is_none());
    assert_eq!(harness.identity.current(), IdentityStatus::Unregistered);
}

#[tokio::test]
async fn revoking_another_device_keeps_the_local_identity() {
    let db = setup_db().await;
    let transport = FakeIdentityTransport::new();
    let (harness, identity) = registered_harness(&db, transport.clone()).await;
    harness
        .authenticator
        .authenticate_once(&identity)
        .await
        .unwrap();
    let other = Uuid::new_v4();

    harness.authenticator.revoke_device(other).await.unwrap();

    assert_eq!(transport.revoked.lock().unwrap().as_slice(), &[other]);
    assert!(harness.session.get().is_some());
    assert_eq!(harness.identity.registered(), Some(identity));
}

#[tokio::test]
async fn link_is_rejected_when_already_registered() {
    let db = setup_db().await;
    let (harness, _) = registered_harness(&db, FakeIdentityTransport::new()).await;

    let result = harness.authenticator.link(Uuid::new_v4(), "tok").await;

    assert!(matches!(result, Err(AuthError::WrongState)));
    assert_eq!(harness.transport.count("link_device"), 0);
}
