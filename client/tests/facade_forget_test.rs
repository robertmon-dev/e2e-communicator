mod fixtures;

use serde_json::json;
use uuid::Uuid;

use client::adapters::r#in::ipc::facade::Facade;
use client::adapters::r#in::ipc::protocol::{Outcome, Request};
use client::domain::state::identity::IdentityStatus;
use fixtures::{build_facade, identity_with, setup_db};

async fn call(facade: &Facade, method: &str) -> Outcome {
    facade
        .handle(Request {
            id: json!(1),
            method: method.to_string(),
            params: json!({}),
        })
        .await
        .outcome
}

#[tokio::test]
async fn forget_clears_a_registered_identity() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;
    assert!(matches!(
        call(&harness.facade, "identity.register").await,
        Outcome::Ok { .. }
    ));
    let identity = harness.auth.identity.registered().unwrap();
    harness
        .auth
        .authenticator
        .authenticate_once(&identity)
        .await
        .unwrap();

    let outcome = call(&harness.facade, "identity.forget").await;

    assert!(matches!(outcome, Outcome::Ok { .. }), "got {outcome:?}");
    assert_eq!(
        harness.auth.identity.current(),
        IdentityStatus::Unregistered
    );
    assert!(harness.auth.session.get().is_none());
    assert!(
        harness
            .auth
            .identity_repository
            .get()
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn forget_recovers_a_conflicted_identity() {
    let db = setup_db().await;
    let harness = build_facade(
        &db,
        IdentityStatus::Conflict(identity_with(Uuid::new_v4(), Uuid::new_v4())),
        None,
    )
    .await;

    let outcome = call(&harness.facade, "identity.forget").await;

    assert!(matches!(outcome, Outcome::Ok { .. }), "got {outcome:?}");
    assert_eq!(
        harness.auth.identity.current(),
        IdentityStatus::Unregistered
    );
    assert!(
        harness.auth.transport.calls().is_empty(),
        "forgetting must not contact the relay: a conflicted device has no session to do \
         it with, and the relay has already forgotten it (calls: {:?})",
        harness.auth.transport.calls()
    );
}

#[tokio::test]
async fn forgetting_while_unregistered_is_the_wrong_state() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;

    match call(&harness.facade, "identity.forget").await {
        Outcome::Err { error } => assert_eq!(error.code, -32005, "{}", error.message),
        Outcome::Ok { result } => panic!("expected -32005, got Ok {result}"),
    }
}

#[tokio::test]
async fn forget_then_register_produces_a_fresh_identity() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;

    let first_user: Uuid = match call(&harness.facade, "identity.register").await {
        Outcome::Ok { result } => serde_json::from_value(result["user_id"].clone()).unwrap(),
        Outcome::Err { error } => panic!("register failed: {error:?}"),
    };

    assert!(matches!(
        call(&harness.facade, "identity.forget").await,
        Outcome::Ok { .. }
    ));

    let second_user: Uuid = match call(&harness.facade, "identity.register").await {
        Outcome::Ok { result } => serde_json::from_value(result["user_id"].clone()).unwrap(),
        Outcome::Err { error } => panic!("second register failed: {error:?}"),
    };

    assert_ne!(first_user, second_user, "a fresh identity must be minted");
}
