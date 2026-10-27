mod fixtures;

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use client::adapters::r#in::ipc::facade::Facade;
use client::adapters::r#in::ipc::protocol::{Outcome, Request};
use client::domain::state::identity::IdentityStatus;
use client::domain::state::session::SessionToken;
use fixtures::{build_facade, identity_with, setup_db};

async fn call(facade: &Facade, method: &str, params: Value) -> Outcome {
    facade
        .handle(Request {
            id: json!(1),
            method: method.to_string(),
            params,
        })
        .await
        .outcome
}

fn expect_ok(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Ok { result } => result,
        Outcome::Err { error } => panic!("expected Ok, got {error:?}"),
    }
}

fn expect_code(outcome: Outcome, code: i32) {
    match outcome {
        Outcome::Err { error } => assert_eq!(error.code, code, "message: {}", error.message),
        Outcome::Ok { result } => panic!("expected error {code}, got Ok {result}"),
    }
}

fn session_token() -> SessionToken {
    SessionToken {
        token: "test-session".to_string(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    }
}

#[tokio::test]
async fn identity_get_reports_unregistered_without_ids() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;

    let result = expect_ok(call(&harness.facade, "identity.get", json!({})).await);

    assert_eq!(result["state"], json!("unregistered"));
    assert_eq!(result["user_id"], Value::Null);
    assert_eq!(result["device_id"], Value::Null);
}

#[tokio::test]
async fn identity_get_reports_the_registered_ids() {
    let db = setup_db().await;
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let harness = build_facade(
        &db,
        IdentityStatus::Registered(identity_with(user_id, device_id)),
        None,
    )
    .await;

    let result = expect_ok(call(&harness.facade, "identity.get", json!({})).await);

    assert_eq!(result["state"], json!("registered"));
    assert_eq!(result["user_id"], json!(user_id));
    assert_eq!(result["device_id"], json!(device_id));
}

#[tokio::test]
async fn session_init_exposes_the_identity_state_and_session() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;

    let init = harness
        .facade
        .authenticate_session(json!({ "protocol_version": 2 }))
        .unwrap();

    assert_eq!(init.identity_state, "unregistered");
    assert!(init.user_id.is_none());
    assert!(init.device_id.is_none());
    assert!(!init.authenticated);

    harness.auth.session.set(session_token());
    let init = harness
        .facade
        .authenticate_session(json!({ "protocol_version": 2 }))
        .unwrap();
    assert!(init.authenticated);
}

#[tokio::test]
async fn identity_register_creates_an_identity_and_refuses_a_second_time() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;

    let result = expect_ok(call(&harness.facade, "identity.register", json!({})).await);

    let user_id: Uuid = serde_json::from_value(result["user_id"].clone()).unwrap();
    assert_eq!(
        harness.auth.identity.registered().unwrap().user_id,
        user_id,
        "the state must reflect the new identity"
    );
    assert_eq!(
        harness
            .auth
            .identity_repository
            .get()
            .await
            .unwrap()
            .unwrap()
            .user_id,
        user_id,
        "the identity must be persisted"
    );

    expect_code(
        call(&harness.facade, "identity.register", json!({})).await,
        -32005,
    );
}

#[tokio::test]
async fn identity_link_uses_the_token_and_the_given_user_id() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;
    let existing_user = Uuid::new_v4();
    harness
        .auth
        .transport
        .link_tokens
        .lock()
        .unwrap()
        .insert("tok".to_string(), existing_user);

    let seed = common::identity::generate_signing_seed().unwrap();
    let result = expect_ok(
        call(
            &harness.facade,
            "identity.link",
            json!({
                "user_id": existing_user,
                "token": "tok",
                "user_seed": common::identity::encode_token(seed.as_slice()),
            }),
        )
        .await,
    );

    assert_eq!(result["user_id"], json!(existing_user));
    assert_eq!(
        harness.auth.identity.registered().unwrap().user_id,
        existing_user
    );
}

#[tokio::test]
async fn a_rejected_link_maps_to_link_rejected_and_persists_nothing() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;

    let seed = common::identity::generate_signing_seed().unwrap();
    let outcome = call(
        &harness.facade,
        "identity.link",
        json!({
            "user_id": Uuid::new_v4(),
            "token": "unknown",
            "user_seed": common::identity::encode_token(seed.as_slice()),
        }),
    )
    .await;

    expect_code(outcome, -32007);
    assert_eq!(
        harness.auth.identity.current(),
        IdentityStatus::Unregistered
    );
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
async fn device_methods_require_a_session() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;
    expect_ok(call(&harness.facade, "identity.register", json!({})).await);

    expect_code(
        call(&harness.facade, "devices.create_link_token", json!({})).await,
        -32008,
    );
    expect_code(
        call(&harness.facade, "devices.list", json!({})).await,
        -32008,
    );
    expect_code(
        call(
            &harness.facade,
            "devices.revoke",
            json!({ "device_id": Uuid::new_v4() }),
        )
        .await,
        -32008,
    );
}

#[tokio::test]
async fn the_device_flow_works_with_a_session() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;
    expect_ok(call(&harness.facade, "identity.register", json!({})).await);
    let identity = harness.auth.identity.registered().unwrap();
    harness
        .auth
        .authenticator
        .authenticate_once(&identity)
        .await
        .unwrap();

    let link = expect_ok(call(&harness.facade, "devices.create_link_token", json!({})).await);
    assert_eq!(link["user_id"], json!(identity.user_id));
    assert!(
        link["token"]
            .as_str()
            .is_some_and(|token| !token.is_empty())
    );

    let devices = expect_ok(call(&harness.facade, "devices.list", json!({})).await);
    assert_eq!(devices.as_array().unwrap().len(), 1);
    assert_eq!(devices[0]["device_id"], json!(identity.device_id));
    assert!(
        devices[0]["keys"]["signing_key"].as_str().is_some(),
        "public keys must serialise as base64url strings, got {}",
        devices[0]["keys"]
    );

    expect_ok(
        call(
            &harness.facade,
            "devices.revoke",
            json!({ "device_id": identity.device_id }),
        )
        .await,
    );

    assert_eq!(
        harness.auth.identity.current(),
        IdentityStatus::Unregistered
    );
    assert!(harness.auth.session.get().is_none());
}

#[tokio::test]
async fn write_rpcs_need_a_registered_identity() {
    let db = setup_db().await;
    let harness = build_facade(&db, IdentityStatus::Unregistered, None).await;

    expect_code(
        call(
            &harness.facade,
            "messages.send",
            json!({ "chat_id": Uuid::new_v4(), "text": "hi" }),
        )
        .await,
        -32005,
    );
    expect_code(
        call(
            &harness.facade,
            "messages.delete",
            json!({ "message_id": Uuid::new_v4() }),
        )
        .await,
        -32005,
    );
}
