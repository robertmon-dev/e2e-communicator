mod fixtures;

use std::sync::Arc;

use client::adapters::r#in::ipc::facade::Facade;
use client::adapters::r#in::ipc::protocol::{Outcome, Request};
use client::domain::state::identity::IdentityStatus;
use fixtures::setup_db;
use serde_json::json;
use uuid::Uuid;

async fn build_facade(
    db: &Arc<sea_orm::DatabaseConnection>,
    user_id: Uuid,
    device_id: Uuid,
) -> Arc<Facade> {
    fixtures::build_facade(
        db,
        IdentityStatus::Registered(fixtures::identity_with(user_id, device_id)),
        None,
    )
    .await
    .facade
}

#[tokio::test]
async fn identity_get_returns_the_local_identity() {
    let db = setup_db().await;
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let facade = build_facade(&db, user_id, device_id).await;

    let response = facade
        .handle(Request {
            id: json!(1),
            method: "identity.get".to_string(),
            params: json!({}),
        })
        .await;

    match response.outcome {
        Outcome::Ok { result } => {
            assert_eq!(result["user_id"], json!(user_id));
            assert_eq!(result["device_id"], json!(device_id));
        }
        Outcome::Err { error } => panic!("expected Ok, got error {error:?}"),
    }
}

#[tokio::test]
async fn an_unknown_method_returns_method_not_found() {
    let db = setup_db().await;
    let facade = build_facade(&db, Uuid::new_v4(), Uuid::new_v4()).await;

    let response = facade
        .handle(Request {
            id: json!(1),
            method: "nope.nope".to_string(),
            params: json!({}),
        })
        .await;

    match response.outcome {
        Outcome::Err { error } => assert_eq!(error.code, -32601),
        Outcome::Ok { .. } => panic!("expected an error"),
    }
}

#[tokio::test]
async fn messages_send_rejects_empty_text_with_a_stable_error_code() {
    let db = setup_db().await;
    let facade = build_facade(&db, Uuid::new_v4(), Uuid::new_v4()).await;

    let response = facade
        .handle(Request {
            id: json!(1),
            method: "messages.send".to_string(),
            params: json!({ "chat_id": Uuid::new_v4(), "text": "   " }),
        })
        .await;

    match response.outcome {
        Outcome::Err { error } => assert_eq!(error.code, -32001),
        Outcome::Ok { .. } => panic!("expected an error"),
    }
}

#[tokio::test]
async fn chats_get_returns_not_found_for_an_unknown_chat() {
    let db = setup_db().await;
    let facade = build_facade(&db, Uuid::new_v4(), Uuid::new_v4()).await;

    let response = facade
        .handle(Request {
            id: json!(1),
            method: "chats.get".to_string(),
            params: json!({ "chat_id": Uuid::new_v4() }),
        })
        .await;

    match response.outcome {
        Outcome::Err { error } => assert_eq!(error.code, -32002),
        Outcome::Ok { .. } => panic!("expected an error"),
    }
}
