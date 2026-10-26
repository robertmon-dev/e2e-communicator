mod fixtures;

use std::sync::Arc;

use serde_json::{Value, json};
use uuid::Uuid;

use client::adapters::r#in::ipc::gate::{CODE_BAD_PIN, CODE_LOCKED, Gate};
use client::adapters::r#in::ipc::protocol::{Outcome, Request};
use client::core::initializers::settings::Settings;
use client::domain::state::facade_events::{FacadeEvent, facade_event_bus};
use client::domain::state::identity::IdentityStatus;
use client::domain::state::vault::{VaultState, VaultStatus};
use fixtures::{build_facade, setup_db};

const PIN: &str = "19283746";
const SALT: &str = "u8X/r7O9dGqW2+eFzL5aKjH1P6Y=";

fn settings_for(db_path: &std::path::Path, token: Option<&str>) -> Arc<Settings> {
    Arc::new(
        serde_json::from_value(json!({
            "environment": "test",
            "logging": { "logging_level": "none" },
            "database": {
                "url": format!("sqlite://{}?mode=rwc", db_path.display()),
                "encryption_key": Value::Null,
                "salt": SALT,
            },
            "relay": { "endpoint": "http://127.0.0.1:1" },
            "facade": { "socket_path": Value::Null, "session_token": token },
        }))
        .expect("test settings deserialize"),
    )
}

fn temp_db() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("gate-test-{}.db", Uuid::new_v4()))
}

fn locked_gate(db_path: &std::path::Path, token: Option<&str>) -> (Arc<Gate>, Arc<VaultState>) {
    let (_tx, rx) = tokio::sync::watch::channel(false);
    std::mem::forget(_tx);
    let vault = Arc::new(VaultState::default());
    let gate = Arc::new(Gate::locked(
        settings_for(db_path, token),
        facade_event_bus(),
        rx,
        vault.clone(),
    ));
    (gate, vault)
}

async fn call(gate: &Gate, method: &str, params: Value) -> Outcome {
    gate.handle(Request {
        id: json!(1),
        method: method.to_string(),
        params,
    })
    .await
    .outcome
}

fn expect_code(outcome: Outcome, code: i32) {
    match outcome {
        Outcome::Err { error } => assert_eq!(error.code, code, "message: {}", error.message),
        Outcome::Ok { result } => panic!("expected error {code}, got Ok {result}"),
    }
}

fn expect_ok(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Ok { result } => result,
        Outcome::Err { error } => panic!("expected Ok, got {error:?}"),
    }
}

#[tokio::test]
async fn a_locked_gate_refuses_every_method_but_the_vault_ones() {
    let path = temp_db();
    let (gate, _vault) = locked_gate(&path, None);

    assert_eq!(gate.vault_status(), VaultStatus::Locked);

    for method in [
        "identity.get",
        "chats.list",
        "devices.list",
        "messages.send",
    ] {
        expect_code(call(&gate, method, json!({})).await, CODE_LOCKED);
    }

    let status = expect_ok(call(&gate, "vault.status", json!({})).await);
    assert_eq!(status["state"], json!("locked"));
}

#[tokio::test]
async fn a_locked_handshake_says_so_without_inventing_an_identity() {
    let path = temp_db();
    let (gate, _vault) = locked_gate(&path, None);

    let init = gate
        .authenticate_session(json!({ "protocol_version": 2 }))
        .await
        .expect("a locked handshake is still accepted");

    assert_eq!(init["vault_state"], json!("locked"));
    assert_eq!(init["identity_state"], json!("locked"));
    assert_eq!(init["user_id"], Value::Null);
    assert_eq!(init["device_id"], Value::Null);
    assert_eq!(init["authenticated"], json!(false));
}

#[tokio::test]
async fn a_locked_handshake_still_enforces_the_protocol_and_the_token() {
    let path = temp_db();
    let (gate, _vault) = locked_gate(&path, Some("s3cret"));

    let rejected = gate
        .authenticate_session(json!({ "protocol_version": 99, "token": "s3cret" }))
        .await;
    assert_eq!(rejected.err().map(|e| e.code), Some(-32003));

    let unauthorized = gate
        .authenticate_session(json!({ "protocol_version": 2, "token": "wrong" }))
        .await;
    assert_eq!(unauthorized.err().map(|e| e.code), Some(-32004));

    gate.authenticate_session(json!({ "protocol_version": 2, "token": "s3cret" }))
        .await
        .expect("the right token gets in even while locked");
}

#[tokio::test]
async fn a_malformed_pin_is_refused_without_opening_anything() {
    let path = temp_db();
    let (gate, vault) = locked_gate(&path, None);

    for bad in ["", "1234", "1234567a", "123456789"] {
        expect_code(
            call(&gate, "vault.unlock", json!({ "pin": bad })).await,
            CODE_BAD_PIN,
        );
    }

    assert_eq!(vault.current(), VaultStatus::Locked);
    assert!(!path.exists(), "a refused PIN must not create the database");
}

#[tokio::test]
async fn unlock_params_must_carry_a_pin() {
    let path = temp_db();
    let (gate, _vault) = locked_gate(&path, None);

    expect_code(call(&gate, "vault.unlock", json!({})).await, -32602);
}

#[tokio::test]
async fn the_right_pin_unlocks_the_gate_and_the_facade_answers() {
    let path = temp_db();
    let (gate, vault) = locked_gate(&path, None);

    expect_ok(call(&gate, "vault.unlock", json!({ "pin": PIN })).await);

    assert_eq!(vault.current(), VaultStatus::Unlocked);
    assert!(path.exists(), "the database must have been created");

    let identity = expect_ok(call(&gate, "identity.get", json!({})).await);
    assert_eq!(
        identity["state"],
        json!("unregistered"),
        "a fresh database has no identity yet"
    );

    let status = expect_ok(call(&gate, "vault.status", json!({})).await);
    assert_eq!(status["state"], json!("unlocked"));

    for handle in gate.take_handles().await {
        handle.abort();
    }
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn unlocking_twice_is_refused() {
    let path = temp_db();
    let (gate, _vault) = locked_gate(&path, None);

    expect_ok(call(&gate, "vault.unlock", json!({ "pin": PIN })).await);
    expect_code(
        call(&gate, "vault.unlock", json!({ "pin": PIN })).await,
        CODE_LOCKED,
    );

    for handle in gate.take_handles().await {
        handle.abort();
    }
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_wrong_pin_cannot_open_an_existing_database() {
    let path = temp_db();

    let (first, _vault) = locked_gate(&path, None);
    expect_ok(call(&first, "vault.unlock", json!({ "pin": PIN })).await);
    for handle in first.take_handles().await {
        handle.abort();
    }
    drop(first);

    let (second, vault) = locked_gate(&path, None);
    expect_code(
        call(&second, "vault.unlock", json!({ "pin": "11112222" })).await,
        CODE_BAD_PIN,
    );
    assert_eq!(
        vault.current(),
        VaultStatus::Locked,
        "a wrong PIN must leave the vault shut"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn an_unlocked_gate_delegates_to_the_facade_and_reports_its_identity() {
    let db = setup_db().await;
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let harness = build_facade(
        &db,
        IdentityStatus::Registered(fixtures::identity_with(user_id, device_id)),
        None,
    )
    .await;

    let gate = Gate::unlocked(harness.facade, None).await;

    assert_eq!(gate.vault_status(), VaultStatus::Unlocked);

    let identity = expect_ok(call(&gate, "identity.get", json!({})).await);
    assert_eq!(identity["user_id"], json!(user_id));

    let init = gate
        .authenticate_session(json!({ "protocol_version": 2 }))
        .await
        .unwrap();
    assert_eq!(init["vault_state"], json!("unlocked"));
    assert_eq!(init["identity_state"], json!("registered"));
}

#[tokio::test]
async fn unlocking_announces_itself_so_a_frontend_can_stop_asking() {
    let path = temp_db();
    let (_tx, rx) = tokio::sync::watch::channel(false);
    std::mem::forget(_tx);
    let events = facade_event_bus();
    let mut received = events.subscribe();
    let vault = Arc::new(VaultState::default());

    let gate = Gate::locked(settings_for(&path, None), events, rx, vault);

    expect_ok(call(&gate, "vault.unlock", json!({ "pin": PIN })).await);

    let event = received
        .try_recv()
        .expect("a vault event must be published");
    assert!(
        matches!(event, FacadeEvent::VaultChanged(VaultStatus::Unlocked)),
        "got {event:?}"
    );

    for handle in gate.take_handles().await {
        handle.abort();
    }
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn repeated_wrong_pins_get_slower() {
    let path = temp_db();

    let (first, _vault) = locked_gate(&path, None);
    expect_ok(call(&first, "vault.unlock", json!({ "pin": PIN })).await);
    for handle in first.take_handles().await {
        handle.abort();
    }
    drop(first);

    let (gate, _vault) = locked_gate(&path, None);

    let started = std::time::Instant::now();
    expect_code(
        call(&gate, "vault.unlock", json!({ "pin": "11112222" })).await,
        CODE_BAD_PIN,
    );
    let first_attempt = started.elapsed();

    let started = std::time::Instant::now();
    expect_code(
        call(&gate, "vault.unlock", json!({ "pin": "11112222" })).await,
        CODE_BAD_PIN,
    );
    let second_attempt = started.elapsed();

    assert!(
        second_attempt > first_attempt + std::time::Duration::from_millis(300),
        "a second wrong PIN must cost a delay, so the key space cannot be walked at full \
         speed: first {first_attempt:?}, second {second_attempt:?}"
    );

    let _ = std::fs::remove_file(&path);
}
