mod fixtures;

use std::sync::Arc;

use client::adapters::r#in::ipc::facade::Facade;
use client::adapters::r#in::ipc::facade::utils::CODE_WRONG_STATE;
use client::adapters::r#in::ipc::protocol::{Outcome, Request};
use client::adapters::out::keys::{DeviceKeyStoreRepository, UserIdentityStoreRepository};
use client::domain::state::identity::IdentityStatus;
use common::identity::{decode_contact_code, user_fingerprint};
use fixtures::setup_db;
use serde_json::{Value, json};
use uuid::Uuid;

async fn registered_facade(
    db: &Arc<sea_orm::DatabaseConnection>,
) -> (fixtures::FacadeHarness, Uuid, Uuid) {
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    fixtures::seed_user(db, user_id).await;

    let harness = fixtures::build_facade(
        db,
        IdentityStatus::Registered(fixtures::identity_with(user_id, device_id)),
        None,
    )
    .await;

    harness
        .auth
        .device_keys
        .get_or_create_keys(device_id)
        .await
        .unwrap();

    (harness, user_id, device_id)
}

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

#[tokio::test]
async fn contact_code_decodes_to_local_identity_and_fingerprint() {
    let db = setup_db().await;
    let (harness, user_id, _device_id) = registered_facade(&db).await;

    let own_key = harness
        .auth
        .user_identity
        .get_or_create(user_id)
        .await
        .unwrap();
    let expected_fingerprint = user_fingerprint(&own_key);

    let outcome = call(&harness.facade, "invitations.contact_code", json!({})).await;

    match outcome {
        Outcome::Ok { result } => {
            assert_eq!(result["user_id"], json!(user_id));
            let code = result["code"].as_str().expect("code is a string");
            let decoded = decode_contact_code(code).expect("code decodes");
            assert_eq!(decoded.user_id, user_id);
            assert_eq!(decoded.fingerprint, expected_fingerprint);
        }
        Outcome::Err { error } => panic!("expected Ok, got error {error:?}"),
    }
}

#[tokio::test]
async fn create_list_and_cancel_an_invitation() {
    let db = setup_db().await;
    let (harness, _user_id, _device_id) = registered_facade(&db).await;
    let contact = harness.seed_contact(1).await;

    let created = call(
        &harness.facade,
        "invitations.create",
        json!({ "code": contact.contact_code, "label": "Ola" }),
    )
    .await;

    let invitation_id = match created {
        Outcome::Ok { result } => result["id"]
            .as_str()
            .expect("invitation id is a string")
            .to_string(),
        Outcome::Err { error } => panic!("expected Ok, got error {error:?}"),
    };

    let listed = call(&harness.facade, "invitations.list", json!({})).await;
    match listed {
        Outcome::Ok { result } => {
            let items = result.as_array().expect("list returns an array");
            assert_eq!(items.len(), 1);
            assert_eq!(items[0]["id"], json!(invitation_id));
        }
        Outcome::Err { error } => panic!("expected Ok, got error {error:?}"),
    }

    let cancelled = call(
        &harness.facade,
        "invitations.cancel",
        json!({ "invitation_id": invitation_id }),
    )
    .await;
    match cancelled {
        Outcome::Ok { result } => assert_eq!(result["status"], json!(3)),
        Outcome::Err { error } => panic!("expected Ok, got error {error:?}"),
    }
}

#[tokio::test]
async fn accepting_an_unknown_invitation_is_a_wrong_state_error() {
    let db = setup_db().await;
    let (harness, _user_id, _device_id) = registered_facade(&db).await;

    let outcome = call(
        &harness.facade,
        "invitations.accept",
        json!({ "invitation_id": Uuid::new_v4() }),
    )
    .await;

    match outcome {
        Outcome::Err { error } => assert_eq!(error.code, CODE_WRONG_STATE),
        Outcome::Ok { .. } => panic!("expected an error"),
    }
}

#[tokio::test]
async fn create_group_returns_an_active_chat() {
    let db = setup_db().await;
    let (harness, _user_id, _device_id) = registered_facade(&db).await;

    let outcome = call(
        &harness.facade,
        "chats.create_group",
        json!({ "name": "Team" }),
    )
    .await;

    match outcome {
        Outcome::Ok { result } => assert_eq!(result["is_active"], json!(true)),
        Outcome::Err { error } => panic!("expected Ok, got error {error:?}"),
    }
}

#[tokio::test]
async fn safety_number_and_verify_for_a_pinned_contact() {
    let db = setup_db().await;
    let (harness, _user_id, _device_id) = registered_facade(&db).await;
    let contact = harness.seed_contact(1).await;

    let created = call(
        &harness.facade,
        "invitations.create",
        json!({ "code": contact.contact_code, "label": "Ola" }),
    )
    .await;
    assert!(matches!(created, Outcome::Ok { .. }), "invite must succeed");

    let safety = call(
        &harness.facade,
        "users.safety_number",
        json!({ "user_id": contact.user_id }),
    )
    .await;

    match safety {
        Outcome::Ok { result } => {
            let number = result["safety_number"]
                .as_str()
                .expect("safety number is a string");
            let groups: Vec<&str> = number.split(' ').collect();
            assert_eq!(groups.len(), 6);
            for group in groups {
                assert_eq!(group.len(), 5);
                assert!(group.chars().all(|c| c.is_ascii_digit()));
            }
            assert!(result["verified_at"].is_null());
        }
        Outcome::Err { error } => panic!("expected Ok, got error {error:?}"),
    }

    let verified = call(
        &harness.facade,
        "users.verify",
        json!({ "user_id": contact.user_id }),
    )
    .await;

    match verified {
        Outcome::Ok { result } => assert!(!result["verified_at"].is_null()),
        Outcome::Err { error } => panic!("expected Ok, got error {error:?}"),
    }
}

#[tokio::test]
async fn safety_number_for_an_unpinned_contact_is_a_wrong_state_error() {
    let db = setup_db().await;
    let (harness, _user_id, _device_id) = registered_facade(&db).await;

    let outcome = call(
        &harness.facade,
        "users.safety_number",
        json!({ "user_id": Uuid::new_v4() }),
    )
    .await;

    match outcome {
        Outcome::Err { error } => assert_eq!(error.code, CODE_WRONG_STATE),
        Outcome::Ok { .. } => panic!("expected an error"),
    }
}

#[tokio::test]
async fn create_invitation_with_an_invalid_code_is_rejected() {
    let db = setup_db().await;
    let (harness, _user_id, _device_id) = registered_facade(&db).await;

    let outcome = call(
        &harness.facade,
        "invitations.create",
        json!({ "code": "not-a-valid-code", "label": "Ola" }),
    )
    .await;

    match outcome {
        Outcome::Err { error } => assert_eq!(error.code, -32602),
        Outcome::Ok { .. } => panic!("expected an error"),
    }
}
