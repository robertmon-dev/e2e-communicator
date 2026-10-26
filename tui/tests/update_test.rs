use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use tui::app::state::{App, ConnectionStatus, InputMode, PendingKind, View};
use tui::app::update::{Event, chats_list_request, messages_list_request, update};
use tui::ipc::connection::HANDSHAKE_ID;
use tui::ipc::protocol::{Outcome, Request, RpcError};

fn ok(result: serde_json::Value) -> Outcome {
    Outcome::Ok { result }
}

fn err(code: i32, message: &str) -> Outcome {
    Outcome::Err {
        error: RpcError {
            code,
            message: message.to_string(),
        },
    }
}

fn session_init_result(state: &str, authenticated: bool) -> serde_json::Value {
    json!({
        "protocol_version": 2,
        "identity_state": state,
        "user_id": null,
        "device_id": null,
        "connectivity": "Online",
        "authenticated": authenticated
    })
}

fn locked_session_init() -> serde_json::Value {
    json!({
        "protocol_version": 2,
        "vault_state": "locked",
        "identity_state": "locked",
        "user_id": null,
        "device_id": null,
        "connectivity": "unknown",
        "authenticated": false
    })
}

fn handshake(app: &mut App, result: serde_json::Value) -> Vec<Request> {
    update(app, Event::Connected);
    update(
        app,
        Event::Response {
            id: HANDSHAKE_ID,
            outcome: ok(result),
        },
    )
}

fn said(app: &App, text: &str) -> bool {
    app.stream.iter().any(|line| line.text.contains(text))
}

fn text_of(key: &str) -> String {
    tui::i18n::t(key)
}

fn prefix(key: &str) -> String {
    tui::i18n::t(key)
        .split('{')
        .next()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn it_starts_disconnected_and_shows_the_stream() {
    let app = App::new();

    assert!(matches!(
        app.connection,
        ConnectionStatus::Disconnected { .. }
    ));
    assert!(matches!(app.view, View::Stream));
    assert!(!app.should_quit);
}

#[test]
fn connecting_and_handshaking_fills_the_state() {
    let mut app = App::new();

    update(&mut app, Event::Connected);
    assert!(matches!(app.connection, ConnectionStatus::Connected));

    update(
        &mut app,
        Event::Response {
            id: HANDSHAKE_ID,
            outcome: ok(session_init_result("unregistered", false)),
        },
    );

    assert_eq!(app.identity.state, "unregistered");
    assert!(!app.authenticated);
    assert_eq!(app.connectivity, "Online");
    assert!(
        app.stream
            .iter()
            .any(|line| line.text.contains(":register")),
        "an unregistered client must be told how to register: {:?}",
        app.stream
    );
}

#[test]
fn a_disconnect_clears_the_session_flag_and_is_logged() {
    let mut app = App::new();
    update(&mut app, Event::Connected);
    update(
        &mut app,
        Event::Response {
            id: HANDSHAKE_ID,
            outcome: ok(session_init_result("registered", true)),
        },
    );
    assert!(app.authenticated);

    update(
        &mut app,
        Event::Disconnected {
            reason: "the client closed the socket".to_string(),
        },
    );

    assert!(matches!(
        app.connection,
        ConnectionStatus::Disconnected { .. }
    ));
    assert!(!app.authenticated, "a dead socket means no usable session");
}

#[test]
fn a_quit_signal_goes_through_the_same_path_as_the_command() {
    let mut app = App::new();
    assert!(!app.should_quit);

    let requests = update(&mut app, Event::Quit);

    assert!(
        app.should_quit,
        "a signal must ask the loop to exit, so the terminal is restored"
    );
    assert!(requests.is_empty());
}

#[test]
fn a_rejected_protocol_version_stops_retrying() {
    let mut app = App::new();
    update(&mut app, Event::Connected);

    update(
        &mut app,
        Event::Response {
            id: HANDSHAKE_ID,
            outcome: err(-32003, "unsupported protocol version: 2"),
        },
    );

    assert!(app.protocol_mismatch);
    assert!(
        app.stream
            .iter()
            .any(|line| line.text.contains(&prefix("error.protocol_mismatch"))),
        "the mismatch must be explained: {:?}",
        app.stream
    );
}

#[test]
fn identity_and_session_notifications_update_the_status_bar() {
    let mut app = App::new();
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    update(
        &mut app,
        Event::Notification {
            method: "identity.changed".to_string(),
            params: json!({ "state": "registered", "user_id": user_id, "device_id": device_id }),
        },
    );
    update(
        &mut app,
        Event::Notification {
            method: "session.changed".to_string(),
            params: json!({ "authenticated": true }),
        },
    );
    update(
        &mut app,
        Event::Notification {
            method: "connectivity.changed".to_string(),
            params: json!("Offline"),
        },
    );

    assert_eq!(app.identity.user_id, Some(user_id));
    assert!(app.identity.is_registered());
    assert!(app.authenticated);
    assert_eq!(app.connectivity, "Offline");
}

#[test]
fn a_conflicted_identity_is_told_about_forget() {
    let mut app = App::new();

    update(
        &mut app,
        Event::Notification {
            method: "identity.changed".to_string(),
            params: json!({
                "state": "conflict",
                "user_id": Uuid::new_v4(),
                "device_id": Uuid::new_v4()
            }),
        },
    );

    assert!(app.identity.is_conflicted());
    assert!(
        app.stream.iter().any(|line| line.text.contains(":forget")),
        "the only way out of a conflict must be offered: {:?}",
        app.stream
    );
}

#[test]
fn a_devices_list_response_is_routed_and_switches_the_view() {
    let mut app = App::new();
    let id = app.track(PendingKind::DevicesList);

    update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!([{
                "device_id": Uuid::new_v4(),
                "keys": { "signing_key": "a".repeat(43), "agreement_key": "b".repeat(43) },
                "registered_at": "2026-10-24T12:55:40.855Z"
            }])),
        },
    );

    assert_eq!(app.devices.len(), 1);
    assert!(matches!(app.view, View::Devices));
    assert!(
        app.pending.is_empty(),
        "a handled response stops being pending"
    );
}

#[test]
fn a_link_token_response_opens_the_qr_view_and_counts_down() {
    let mut app = App::new();
    let id = app.track(PendingKind::LinkToken);
    let expires_at = Utc::now() + Duration::seconds(90);

    update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!({
                "user_id": Uuid::new_v4(),
                "token": "kJ3n9x",
                "expires_at": expires_at
            })),
        },
    );

    assert!(matches!(app.view, View::LinkToken));
    let remaining = app.link_token_remaining().expect("a countdown");
    assert!(
        remaining.num_seconds() > 80 && remaining.num_seconds() <= 90,
        "got {remaining:?}"
    );
}

#[test]
fn an_expired_link_token_reports_no_time_left() {
    let mut app = App::new();
    let id = app.track(PendingKind::LinkToken);

    update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!({
                "user_id": Uuid::new_v4(),
                "token": "kJ3n9x",
                "expires_at": Utc::now() - Duration::seconds(5)
            })),
        },
    );
    update(&mut app, Event::Tick);

    assert_eq!(
        app.link_token_remaining().map(|left| left.num_seconds()),
        Some(0),
        "an expired token must clamp to zero, not go negative"
    );
}

#[test]
fn an_rpc_error_is_explained_in_the_stream() {
    let mut app = App::new();
    let id = app.track(PendingKind::DevicesList);

    update(
        &mut app,
        Event::Response {
            id,
            outcome: err(-32008, "no session"),
        },
    );

    let text = app
        .stream
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("-32008"),
        "the code must stay visible: {text}"
    );
    assert!(
        text.contains(&text_of("error.no_session")),
        "the code must be explained in plain words: {text}"
    );
    assert!(
        app.pending.is_empty(),
        "a failed request must stop being pending too, or it leaks forever"
    );
}

#[test]
fn an_unknown_error_code_is_shown_verbatim() {
    let mut app = App::new();
    let id = app.track(PendingKind::ChatsList);

    update(
        &mut app,
        Event::Response {
            id,
            outcome: err(-31999, "something odd"),
        },
    );

    let text = app
        .stream
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("-31999") && text.contains("something odd"),
        "got {text}"
    );
}

#[test]
fn a_malformed_line_is_kept_raw_even_in_readable_mode() {
    let mut app = App::new();
    assert!(!app.raw_stream);

    update(
        &mut app,
        Event::Malformed {
            line: "{ nonsense }".to_string(),
        },
    );

    assert!(
        app.stream
            .iter()
            .any(|line| line.text.contains("{ nonsense }")),
        "a protocol mismatch must be visible verbatim: {:?}",
        app.stream
    );
}

#[test]
fn the_stream_is_capped_so_a_long_session_cannot_grow_without_bound() {
    let mut app = App::new();

    for index in 0..(App::STREAM_CAPACITY + 50) {
        update(
            &mut app,
            Event::Malformed {
                line: format!("line {index}"),
            },
        );
    }

    assert_eq!(app.stream.len(), App::STREAM_CAPACITY);
    assert!(
        app.stream
            .back()
            .unwrap()
            .text
            .contains(&format!("line {}", App::STREAM_CAPACITY + 49)),
        "the newest line must survive"
    );
}

#[test]
fn a_message_received_for_the_focused_chat_is_appended() {
    let mut app = App::new();
    let chat_id = Uuid::new_v4();
    app.view = View::Conversation(chat_id);

    update(
        &mut app,
        Event::Notification {
            method: "message.received".to_string(),
            params: json!({
                "id": Uuid::new_v4(),
                "author_id": Uuid::new_v4(),
                "chat_id": chat_id,
                "history": { "2026-10-24T12:00:00Z": { "id": Uuid::new_v4(), "text": "hej", "attachments": [], "reactions": [] } },
                "seen_by": {},
                "created_at": "2026-10-24T12:00:00Z",
                "updated_at": "2026-10-24T12:00:00Z",
                "available_to": null, "hidden": false, "is_reply": false, "reply_to_id": null
            }),
        },
    );

    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages[0].text(), "hej");
}

#[test]
fn a_message_for_another_chat_is_logged_but_not_appended() {
    let mut app = App::new();
    app.view = View::Conversation(Uuid::new_v4());

    update(
        &mut app,
        Event::Notification {
            method: "message.received".to_string(),
            params: json!({
                "id": Uuid::new_v4(),
                "author_id": Uuid::new_v4(),
                "chat_id": Uuid::new_v4(),
                "history": { "2026-10-24T12:00:00Z": { "id": Uuid::new_v4(), "text": "gdzie indziej", "attachments": [], "reactions": [] } },
                "seen_by": {},
                "created_at": "2026-10-24T12:00:00Z",
                "updated_at": "2026-10-24T12:00:00Z",
                "available_to": null, "hidden": false, "is_reply": false, "reply_to_id": null
            }),
        },
    );

    assert!(app.messages.is_empty());
    assert!(
        app.stream
            .iter()
            .any(|line| line.text.contains(&prefix("notify.message_elsewhere")))
    );
}

#[test]
fn a_disconnect_voids_requests_that_were_already_in_flight() {
    let mut app = App::new();
    update(&mut app, Event::Connected);
    app.track(PendingKind::DevicesList);
    assert!(!app.pending.is_empty());

    update(
        &mut app,
        Event::Disconnected {
            reason: "the client closed the socket".to_string(),
        },
    );

    assert!(
        app.pending.is_empty(),
        "a request the dead facade never answered can never be resolved"
    );
}

#[test]
fn a_registered_handshake_asks_for_the_chat_list() {
    let mut app = App::new();
    update(&mut app, Event::Connected);

    let requests = update(
        &mut app,
        Event::Response {
            id: HANDSHAKE_ID,
            outcome: ok(session_init_result("registered", true)),
        },
    );

    assert_eq!(requests.len(), 1, "got {requests:?}");
    assert_eq!(requests[0].method, "chats.list");
    assert_eq!(
        app.pending.get(&requests[0].id),
        Some(&PendingKind::ChatsList),
        "the request must be tracked so its response can be routed"
    );
}

#[test]
fn a_conflicted_handshake_asks_for_nothing() {
    let mut app = App::new();
    update(&mut app, Event::Connected);

    let requests = update(
        &mut app,
        Event::Response {
            id: HANDSHAKE_ID,
            outcome: ok(session_init_result("conflict", false)),
        },
    );

    assert!(
        requests.is_empty(),
        "a conflicted identity has nothing to list: {requests:?}"
    );
}

#[test]
fn a_sent_message_reloads_the_open_conversation() {
    let mut app = App::new();
    let chat_id = Uuid::new_v4();
    app.view = View::Conversation(chat_id);
    let id = app.track(PendingKind::MessageSend);

    let requests = update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!({ "id": Uuid::new_v4() })),
        },
    );

    assert_eq!(requests.len(), 1, "got {requests:?}");
    assert_eq!(requests[0].method, "messages.list");
    assert_eq!(
        requests[0].params["filter"]["chat_id"],
        json!(chat_id),
        "the reload must be scoped to the open chat"
    );
}

#[test]
fn a_notification_for_the_open_chat_lands_in_time_order() {
    let mut app = App::new();
    let chat_id = Uuid::new_v4();
    app.view = View::Conversation(chat_id);

    let message = |text: &str, at: &str| {
        json!({
            "id": Uuid::new_v4(),
            "author_id": Uuid::new_v4(),
            "chat_id": chat_id,
            "history": { at: { "id": Uuid::new_v4(), "text": text, "attachments": [], "reactions": [] } },
            "seen_by": {},
            "created_at": at,
            "updated_at": at,
            "available_to": null, "hidden": false, "is_reply": false, "reply_to_id": null
        })
    };

    for (text, at) in [
        ("druga", "2026-10-24T12:02:00Z"),
        ("pierwsza", "2026-10-24T12:01:00Z"),
        ("trzecia", "2026-10-24T12:03:00Z"),
    ] {
        update(
            &mut app,
            Event::Notification {
                method: "message.received".to_string(),
                params: message(text, at),
            },
        );
    }

    let order: Vec<&str> = app.messages.iter().map(|message| message.text()).collect();
    assert_eq!(order, vec!["pierwsza", "druga", "trzecia"]);
}

#[test]
fn a_garbled_session_change_does_not_silently_log_us_out() {
    let mut app = App::new();
    update(
        &mut app,
        Event::Notification {
            method: "session.changed".to_string(),
            params: json!({ "authenticated": true }),
        },
    );
    assert!(app.authenticated);

    update(
        &mut app,
        Event::Notification {
            method: "session.changed".to_string(),
            params: json!({ "nonsense": 1 }),
        },
    );

    assert!(
        app.authenticated,
        "a malformed payload is not the server saying the session ended"
    );
    assert!(
        app.stream
            .iter()
            .any(|line| line.text.contains("session.changed")),
        "it must still be reported: {:?}",
        app.stream
    );
}

#[test]
fn a_garbled_connectivity_change_is_reported_rather_than_dropped() {
    let mut app = App::new();

    update(
        &mut app,
        Event::Notification {
            method: "connectivity.changed".to_string(),
            params: json!({ "not": "a string" }),
        },
    );

    assert_eq!(app.connectivity, "unknown", "the state must not change");
    assert!(
        app.stream
            .iter()
            .any(|line| line.text.contains("connectivity.changed")),
        "a malformed payload must be visible: {:?}",
        app.stream
    );
}

#[test]
fn list_requests_spell_pagination_in_camel_case_as_the_facade_demands() {
    let mut app = App::new();
    let chat_id = Uuid::new_v4();

    for request in [
        chats_list_request(&mut app),
        messages_list_request(&mut app, chat_id),
    ] {
        let pagination = &request.params["pagination"];
        assert_eq!(pagination["currentPage"], json!(0), "{}", request.method);
        assert_eq!(pagination["perPage"], json!(50), "{}", request.method);
        assert_eq!(pagination["totalPages"], json!(1), "{}", request.method);
        assert_eq!(pagination["totalCount"], json!(50), "{}", request.method);
        assert!(
            pagination.get("current_page").is_none(),
            "snake_case is rejected by the facade: {}",
            request.method
        );

        assert_eq!(request.params["filter"]["per_page"], json!(50));
        assert_eq!(request.params["filter"]["search_in_latest"], json!(true));
    }
}

#[test]
fn a_locked_handshake_asks_for_the_pin_and_nothing_else() {
    let mut app = App::new();

    let requests = handshake(&mut app, locked_session_init());

    assert!(app.locked, "the client said its vault is shut");
    assert_eq!(app.view, View::Unlock);
    assert_eq!(app.input, InputMode::Pin(String::new()));
    assert!(
        requests.is_empty(),
        "a locked client answers nothing, so asking is noise: {requests:?}"
    );
    assert!(said(&app, &text_of("unlock.locked")), "{:?}", app.stream);
    assert!(
        !said(&app, &text_of("boot.no_identity")),
        "a shut vault is not a missing identity: {:?}",
        app.stream
    );
}

#[test]
fn the_right_pin_unlocks_the_client_and_it_then_asks_who_it_is() {
    let mut app = App::new();
    handshake(&mut app, locked_session_init());

    let id = app.track(PendingKind::VaultUnlock);
    let requests = update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!({ "state": "unlocked" })),
        },
    );

    assert!(!app.locked);
    assert_eq!(app.view, View::Stream);
    assert_eq!(app.input, InputMode::Idle);
    assert_eq!(
        requests.len(),
        1,
        "the handshake identity was 'locked', so it must be asked again: {requests:?}"
    );
    assert_eq!(requests[0].method, "identity.get");

    let id = requests[0].id;
    let user_id = Uuid::new_v4();
    let requests = update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!({
                "state": "registered",
                "user_id": user_id,
                "device_id": Uuid::new_v4()
            })),
        },
    );

    assert_eq!(app.identity.state, "registered");
    assert_eq!(app.identity.user_id, Some(user_id));
    assert_eq!(
        requests.len(),
        1,
        "a registered identity means the chats can load: {requests:?}"
    );
    assert_eq!(requests[0].method, "chats.list");
}

#[test]
fn an_identity_that_is_still_missing_after_unlocking_is_told_how_to_register() {
    let mut app = App::new();
    handshake(&mut app, locked_session_init());
    let id = app.track(PendingKind::VaultUnlock);
    update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!({ "state": "unlocked" })),
        },
    );

    let id = app.track(PendingKind::IdentityGet);
    let requests = update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!({ "state": "unregistered", "user_id": null, "device_id": null })),
        },
    );

    assert!(requests.is_empty());
    assert!(said(&app, ":register"), "{:?}", app.stream);
}

#[test]
fn a_refused_pin_leaves_the_client_locked() {
    let mut app = App::new();
    handshake(&mut app, locked_session_init());

    let id = app.track(PendingKind::VaultUnlock);
    let requests = update(
        &mut app,
        Event::Response {
            id,
            outcome: err(-32010, "The database could not be opened with that PIN"),
        },
    );

    assert!(app.locked, "a wrong PIN must leave the vault shut");
    assert_eq!(app.view, View::Unlock);
    assert_eq!(app.input, InputMode::Pin(String::new()));
    assert!(requests.is_empty());
    assert!(said(&app, &text_of("error.bad_pin")), "{:?}", app.stream);
}

#[test]
fn a_method_refused_for_being_locked_is_explained() {
    let mut app = App::new();
    handshake(&mut app, locked_session_init());

    let id = app.track(PendingKind::DevicesList);
    update(
        &mut app,
        Event::Response {
            id,
            outcome: err(-32009, "the vault is locked: call vault.unlock first"),
        },
    );

    assert!(said(&app, &text_of("error.locked")), "{:?}", app.stream);
}

#[test]
fn a_vault_notification_follows_the_client_both_ways() {
    let mut app = App::new();
    handshake(&mut app, locked_session_init());

    let requests = update(
        &mut app,
        Event::Notification {
            method: "vault.changed".to_string(),
            params: json!({ "state": "unlocked" }),
        },
    );

    assert!(!app.locked, "another client may open the vault for us");
    assert_eq!(
        requests.len(),
        1,
        "and then this one still has to find out who it is: {requests:?}"
    );

    let requests = update(
        &mut app,
        Event::Notification {
            method: "vault.changed".to_string(),
            params: json!({ "state": "locked" }),
        },
    );

    assert!(app.locked);
    assert_eq!(app.view, View::Unlock);
    assert_eq!(app.input, InputMode::Pin(String::new()));
    assert!(requests.is_empty());
    assert!(said(&app, &text_of("unlock.relocked")), "{:?}", app.stream);
}

#[test]
fn an_unlocked_response_arriving_twice_asks_only_once() {
    let mut app = App::new();
    handshake(&mut app, locked_session_init());

    let id = app.track(PendingKind::VaultUnlock);
    update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!({ "state": "unlocked" })),
        },
    );

    let requests = update(
        &mut app,
        Event::Notification {
            method: "vault.changed".to_string(),
            params: json!({ "state": "unlocked" }),
        },
    );

    assert!(
        requests.is_empty(),
        "the broadcast repeats what the response already said: {requests:?}"
    );
}

#[test]
fn reconnecting_to_an_open_client_leaves_the_unlock_view() {
    let mut app = App::new();
    handshake(&mut app, locked_session_init());
    update(
        &mut app,
        Event::Disconnected {
            reason: "the client closed the socket".to_string(),
        },
    );

    let requests = handshake(&mut app, session_init_result("registered", true));

    assert!(!app.locked);
    assert_eq!(app.view, View::Stream);
    assert_eq!(app.input, InputMode::Idle);
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].method, "chats.list");
}

#[test]
fn a_garbled_identity_is_reported_rather_than_dropped() {
    let mut app = App::new();
    handshake(&mut app, locked_session_init());

    let id = app.track(PendingKind::IdentityGet);
    update(
        &mut app,
        Event::Response {
            id,
            outcome: ok(json!({ "nope": true })),
        },
    );

    assert!(
        said(&app, &prefix("response.bad_identity")),
        "{:?}",
        app.stream
    );
}
