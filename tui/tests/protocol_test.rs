use chrono::{TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;

use tui::ipc::protocol::{
    ChatView, DeviceView, IdentityInfo, LinkTokenView, MessageView, Outcome, Response, SessionInit,
};

#[test]
fn a_successful_response_parses_into_ok() {
    let raw = json!({ "id": 7, "result": { "state": "registered" } }).to_string();
    let response: Response = serde_json::from_str(&raw).unwrap();

    assert_eq!(response.id, json!(7));
    match response.outcome {
        Outcome::Ok { result } => assert_eq!(result["state"], json!("registered")),
        Outcome::Err { error } => panic!("expected Ok, got {error:?}"),
    }
}

#[test]
fn an_error_response_parses_into_err() {
    let raw = json!({ "id": 7, "error": { "code": -32008, "message": "no session" } }).to_string();
    let response: Response = serde_json::from_str(&raw).unwrap();

    match response.outcome {
        Outcome::Err { error } => {
            assert_eq!(error.code, -32008);
            assert_eq!(error.message, "no session");
        }
        Outcome::Ok { result } => panic!("expected Err, got {result}"),
    }
}

#[test]
fn session_init_parses_the_unregistered_shape() {
    let init: SessionInit = serde_json::from_value(json!({
        "protocol_version": 2,
        "identity_state": "unregistered",
        "user_id": null,
        "device_id": null,
        "connectivity": "Online",
        "authenticated": false
    }))
    .unwrap();

    assert_eq!(init.identity_state, "unregistered");
    assert!(init.user_id.is_none());
    assert!(!init.authenticated);
}

#[test]
fn identity_info_parses_both_shapes() {
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let registered: IdentityInfo = serde_json::from_value(json!({
        "state": "registered", "user_id": user_id, "device_id": device_id
    }))
    .unwrap();
    assert_eq!(registered.user_id, Some(user_id));
    assert_eq!(
        registered.short_label(),
        Some(format!("user-{}", &user_id.simple().to_string()[..8]))
    );

    let unregistered: IdentityInfo = serde_json::from_value(json!({
        "state": "unregistered", "user_id": null, "device_id": null
    }))
    .unwrap();
    assert!(unregistered.device_id.is_none());
    assert_eq!(unregistered.short_label(), None);
}

#[test]
fn a_device_parses_with_base64url_keys() {
    let device: DeviceView = serde_json::from_value(json!({
        "device_id": Uuid::new_v4(),
        "keys": {
            "signing_key": "1GPqFQTygHgvEy2N2nXN74gzVjcyNF9ZRDlZgWXJjo4",
            "agreement_key": "Gbg7OmOevwEo9jYHYNrWkdcEnK0j_tONnK185Aaavzc"
        },
        "registered_at": "2026-10-24T12:55:40.855Z"
    }))
    .unwrap();

    assert_eq!(device.keys.signing_key.len(), 43);
    assert_eq!(device.short_id().len(), 8);
}

#[test]
fn a_link_token_hides_its_token_in_debug_output() {
    let token = LinkTokenView {
        user_id: Uuid::new_v4(),
        token: "super-secret-token".to_string(),
        expires_at: Utc::now(),
    };

    let rendered = format!("{token:?}");
    assert!(!rendered.contains("super-secret-token"), "got {rendered}");
    assert!(rendered.contains("<redacted>"));
}

#[test]
fn a_link_token_renders_its_qr_payload_as_the_pair_link_takes() {
    let user_id = Uuid::new_v4();
    let token = LinkTokenView {
        user_id,
        token: "kJ3n9x".to_string(),
        expires_at: Utc::now(),
    };

    let payload: serde_json::Value = serde_json::from_str(&token.qr_payload()).unwrap();
    assert_eq!(payload["user_id"], json!(user_id));
    assert_eq!(payload["token"], json!("kJ3n9x"));
    assert_eq!(
        payload.as_object().unwrap().len(),
        2,
        "the QR must carry only the pair `identity.link` takes, not the whole token: {payload}"
    );
}

#[test]
fn a_chat_takes_its_name_from_the_latest_state() {
    let chat: ChatView = serde_json::from_value(json!({
        "id": Uuid::new_v4(),
        "created_at": "2026-10-01T10:00:00Z",
        "history": {
            "2026-10-01T10:00:00Z": { "name": "stara nazwa", "chat_type": 0, "members": {} },
            "2026-10-02T11:00:00Z": { "name": "anna", "chat_type": 0, "members": {} }
        },
        "messages": {}
    }))
    .unwrap();

    assert_eq!(chat.title(), "anna");
    assert_eq!(chat.member_count(), 0);
}

#[test]
fn a_chat_without_a_name_falls_back_to_its_short_id() {
    let id = Uuid::new_v4();
    let chat: ChatView = serde_json::from_value(json!({
        "id": id,
        "created_at": "2026-10-01T10:00:00Z",
        "history": { "2026-10-01T10:00:00Z": { "name": null, "chat_type": 0, "members": {} } },
        "messages": {}
    }))
    .unwrap();

    assert_eq!(
        chat.title(),
        tui::i18n::tf(
            "ui.chat_fallback_title",
            &[("id", &id.simple().to_string()[..8])],
        )
    );
}

#[test]
fn a_message_takes_its_text_from_the_latest_revision() {
    let message: MessageView = serde_json::from_value(json!({
        "id": Uuid::new_v4(),
        "author_id": Uuid::new_v4(),
        "chat_id": Uuid::new_v4(),
        "history": {
            "2026-10-01T10:00:00Z": { "id": Uuid::new_v4(), "text": "pierwsza wersja", "attachments": [], "reactions": [] },
            "2026-10-01T10:05:00Z": { "id": Uuid::new_v4(), "text": "poprawiona", "attachments": [], "reactions": [] }
        },
        "seen_by": {},
        "created_at": "2026-10-01T10:00:00Z",
        "updated_at": "2026-10-01T10:05:00Z",
        "available_to": null,
        "hidden": false,
        "is_reply": false,
        "reply_to_id": null
    }))
    .unwrap();

    assert_eq!(message.text(), "poprawiona");
    assert_eq!(
        message.created_at,
        Utc.with_ymd_and_hms(2026, 10, 1, 10, 0, 0).unwrap()
    );
}

#[test]
fn an_empty_history_degrades_instead_of_panicking() {
    let message: MessageView = serde_json::from_value(json!({
        "id": Uuid::new_v4(), "author_id": Uuid::new_v4(), "chat_id": Uuid::new_v4(),
        "history": {}, "seen_by": {},
        "created_at": "2026-10-01T10:00:00Z", "updated_at": "2026-10-01T10:00:00Z",
        "available_to": null, "hidden": false, "is_reply": false, "reply_to_id": null
    }))
    .unwrap();

    assert_eq!(message.text(), "");
}

#[test]
fn parse_frame_classifies_responses_notifications_and_rubbish() {
    use tui::ipc::protocol::{Frame, parse_frame};

    let response = parse_frame(r#"{"id":4,"result":{}}"#).unwrap();
    assert!(matches!(response, Frame::Response(_)));

    let error = parse_frame(r#"{"id":4,"error":{"code":-1,"message":"x"}}"#).unwrap();
    assert!(matches!(error, Frame::Response(_)));

    let notification = parse_frame(r#"{"method":"session.changed","params":{}}"#).unwrap();
    assert!(matches!(notification, Frame::Notification(_)));

    let bare = parse_frame(r#"{"method":"session.changed"}"#).unwrap();
    assert!(matches!(bare, Frame::Notification(_)));

    for rubbish in [
        "{ not json }",
        "[]",
        "7",
        r#"{"params":{}}"#,
        r#"{"result":{}}"#,
    ] {
        assert!(
            parse_frame(rubbish).is_err(),
            "{rubbish} must not classify as a frame"
        );
    }
}

#[test]
fn a_request_never_prints_the_session_token_but_does_print_other_params() {
    use tui::ipc::protocol::{Request, SESSION_INIT_METHOD};

    let handshake = Request {
        id: 1,
        method: SESSION_INIT_METHOD.to_string(),
        params: json!({ "protocol_version": 2, "token": "super-secret" }),
    };
    let rendered = format!("{handshake:?}");
    assert!(!rendered.contains("super-secret"), "got {rendered}");
    assert!(rendered.contains("<redacted>"));

    let ordinary = Request {
        id: 2,
        method: "devices.revoke".to_string(),
        params: json!({ "device_id": "abc" }),
    };
    let rendered = format!("{ordinary:?}");
    assert!(
        rendered.contains("device_id"),
        "ordinary params must stay visible: {rendered}"
    );
}

#[test]
fn a_whitespace_only_chat_name_falls_back_to_the_short_id() {
    use tui::ipc::protocol::ChatView;

    let id = Uuid::new_v4();
    let chat: ChatView = serde_json::from_value(json!({
        "id": id,
        "created_at": "2026-10-01T10:00:00Z",
        "history": { "2026-10-01T10:00:00Z": { "name": "   ", "chat_type": 0, "members": {} } },
        "messages": {}
    }))
    .unwrap();

    assert_eq!(
        chat.title(),
        tui::i18n::tf(
            "ui.chat_fallback_title",
            &[("id", &id.simple().to_string()[..8])],
        )
    );
}
