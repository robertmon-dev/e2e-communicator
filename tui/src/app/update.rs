use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::app::state::{
    App, Confirm, ConfirmKind, ConnectionStatus, Focus, InputMode, PendingKind, StreamKind, View,
};
use crate::i18n::{t, tf};
use crate::ipc::connection::HANDSHAKE_ID;
use crate::ipc::protocol::{
    ChatView, ContactCodeView, DeviceView, IdentityInfo, InvitationView, LinkTokenView,
    MessageView, Outcome, PROTOCOL_VERSION, Request, RpcError, SafetyNumberView, SessionInit,
    vault_is_locked,
};

#[derive(Debug)]
pub enum Event {
    Key(ratatui::crossterm::event::KeyEvent),
    Tick,
    Quit,
    Connected,
    Disconnected { reason: String },
    Response { id: u64, outcome: Outcome },
    Notification { method: String, params: Value },
    Malformed { line: String },
}

pub fn update(app: &mut App, event: Event) -> Vec<Request> {
    match event {
        Event::Tick => {
            app.now = Utc::now();
            Vec::new()
        }
        Event::Quit => {
            app.should_quit = true;
            Vec::new()
        }
        Event::Connected => {
            app.connection = ConnectionStatus::Connected;
            app.log(StreamKind::Info, t("stream.connected"));
            app.pending.insert(HANDSHAKE_ID, PendingKind::SessionInit);
            Vec::new()
        }
        Event::Disconnected { reason } => {
            app.connection = ConnectionStatus::Disconnected {
                reason: reason.clone(),
            };
            app.authenticated = false;
            app.pending.clear();
            app.log(
                StreamKind::Problem,
                tf("stream.disconnected", &[("reason", &reason)]),
            );
            Vec::new()
        }
        Event::Malformed { line } => {
            app.log(
                StreamKind::Problem,
                tf("stream.malformed_frame", &[("line", &line)]),
            );
            Vec::new()
        }
        Event::Notification { method, params } => on_notification(app, &method, params),
        Event::Response { id, outcome } => on_response(app, id, outcome),
        Event::Key(key) => crate::app::command::on_key(app, key),
    }
}

fn on_notification(app: &mut App, method: &str, params: Value) -> Vec<Request> {
    if app.raw_stream {
        app.log(StreamKind::Incoming, format!("← {method} {params}"));
    }

    match method {
        "identity.changed" => {
            match serde_json::from_value::<IdentityInfo>(params) {
                Ok(identity) => {
                    let conflicted = identity.is_conflicted();
                    app.identity = identity;
                    if !app.raw_stream {
                        app.log(
                            StreamKind::Incoming,
                            tf("notify.identity", &[("state", &app.identity.state)]),
                        );
                    }
                    if conflicted {
                        explain_conflict(app);
                    }
                }
                Err(err) => app.log(StreamKind::Problem, bad_payload("identity.changed", err)),
            }
            Vec::new()
        }
        "session.changed" => {
            match params["authenticated"].as_bool() {
                Some(authenticated) => {
                    app.authenticated = authenticated;
                    if !app.raw_stream {
                        app.log(
                            StreamKind::Incoming,
                            if authenticated {
                                t("notify.session_on")
                            } else {
                                t("notify.session_none")
                            },
                        );
                    }
                }
                None => app.log(StreamKind::Problem, bad_payload("session.changed", params)),
            }
            Vec::new()
        }
        "connectivity.changed" => {
            match params.as_str() {
                Some(status) => {
                    app.connectivity = status.to_string();
                    if !app.raw_stream {
                        app.log(
                            StreamKind::Incoming,
                            tf("notify.connectivity", &[("status", status)]),
                        );
                    }
                }
                None => app.log(
                    StreamKind::Problem,
                    bad_payload("connectivity.changed", params),
                ),
            }
            Vec::new()
        }
        "vault.changed" => match params["state"].as_str() {
            Some(state) if vault_is_locked(state) => {
                if app.locked {
                    return Vec::new();
                }
                app.log(StreamKind::Problem, t("unlock.relocked"));
                lock(app);
                Vec::new()
            }
            Some(_) => unlock(app),
            None => {
                app.log(StreamKind::Problem, bad_payload("vault.changed", params));
                Vec::new()
            }
        },
        "message.received" => {
            match serde_json::from_value::<MessageView>(params) {
                Ok(message) => {
                    if app.focused_chat() == Some(message.chat_id) {
                        let at = app
                            .messages
                            .partition_point(|existing| existing.created_at <= message.created_at);
                        app.messages.insert(at, message);
                    } else if !app.raw_stream {
                        app.log(
                            StreamKind::Incoming,
                            tf(
                                "notify.message_elsewhere",
                                &[("chat_id", &message.chat_id.to_string())],
                            ),
                        );
                    }
                }
                Err(err) => app.log(StreamKind::Problem, bad_payload("message.received", err)),
            }
            Vec::new()
        }
        "message.delivered" => {
            if !app.raw_stream {
                app.log(StreamKind::Incoming, t("notify.message_delivered"));
            }
            Vec::new()
        }
        "invitation.received" => {
            match serde_json::from_value::<InvitationView>(params) {
                Ok(invitation) => {
                    if !app.raw_stream {
                        app.log(
                            StreamKind::Incoming,
                            tf(
                                "notify.invitation_received",
                                &[("invitation_id", &invitation.id.to_string())],
                            ),
                        );
                    }
                    app.upsert_invitation(invitation);
                }
                Err(err) => app.log(StreamKind::Problem, bad_payload("invitation.received", err)),
            }
            Vec::new()
        }
        "invitation.resolved" => {
            match (
                params["invitation_id"]
                    .as_str()
                    .and_then(|id| Uuid::parse_str(id).ok()),
                params["status"].as_u64(),
            ) {
                (Some(invitation_id), Some(status)) => {
                    app.resolve_invitation(invitation_id, status as u8);
                    if !app.raw_stream {
                        app.log(
                            StreamKind::Incoming,
                            tf(
                                "notify.invitation_resolved",
                                &[("invitation_id", &invitation_id.to_string())],
                            ),
                        );
                    }
                }
                _ => app.log(
                    StreamKind::Problem,
                    bad_payload("invitation.resolved", params),
                ),
            }
            Vec::new()
        }
        "chat.activated" => {
            if !app.raw_stream {
                app.log(StreamKind::Incoming, t("notify.chat_activated"));
            }
            vec![chats_list_request(app)]
        }
        "chat.member_added" => {
            if !app.raw_stream {
                app.log(StreamKind::Incoming, t("notify.chat_member_added"));
            }
            Vec::new()
        }
        "chat.key_exchange_progress" => {
            match (
                params["chat_id"]
                    .as_str()
                    .and_then(|id| Uuid::parse_str(id).ok()),
                params["known"].as_u64(),
                params["total"].as_u64(),
            ) {
                (Some(chat_id), Some(known), Some(total)) => {
                    app.key_exchange
                        .insert(chat_id, (known as usize, total as usize));
                    if !app.raw_stream {
                        app.log(
                            StreamKind::Incoming,
                            tf(
                                "notify.key_exchange_progress",
                                &[("known", &known.to_string()), ("total", &total.to_string())],
                            ),
                        );
                    }
                }
                _ => app.log(
                    StreamKind::Problem,
                    bad_payload("chat.key_exchange_progress", params),
                ),
            }
            Vec::new()
        }
        other => {
            app.log(
                StreamKind::Incoming,
                tf("stream.unknown_notification", &[("method", other)]),
            );
            Vec::new()
        }
    }
}

fn on_response(app: &mut App, id: u64, outcome: Outcome) -> Vec<Request> {
    let kind = app.pending.remove(&id);

    match outcome {
        Outcome::Err { error } => {
            on_error(app, kind, error);
            Vec::new()
        }
        Outcome::Ok { result } => {
            if app.raw_stream {
                app.log(StreamKind::Incoming, format!("← #{id} {result}"));
            }
            match kind {
                Some(PendingKind::SessionInit) => on_session_init(app, result),
                Some(PendingKind::VaultUnlock) => unlock(app),
                Some(PendingKind::IdentityGet) => on_identity(app, result),
                Some(PendingKind::IdentityRegister) => {
                    app.log(StreamKind::Info, t("response.registered"));
                    Vec::new()
                }
                Some(PendingKind::IdentityLink) => {
                    app.log(StreamKind::Info, t("response.linked"));
                    Vec::new()
                }
                Some(PendingKind::IdentityForget) => {
                    app.log(StreamKind::Info, t("response.forgotten"));
                    Vec::new()
                }
                Some(PendingKind::DevicesList) => {
                    match serde_json::from_value::<Vec<DeviceView>>(result) {
                        Ok(devices) => {
                            app.log(
                                StreamKind::Info,
                                tf(
                                    "response.device_count",
                                    &[("count", &devices.len().to_string())],
                                ),
                            );
                            app.devices = devices;
                            app.device_cursor = 0;
                            app.view = View::Devices;
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf("response.bad_device_list", &[("error", &err.to_string())]),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::LinkToken) => {
                    match serde_json::from_value::<LinkTokenView>(result) {
                        Ok(token) => {
                            app.log(StreamKind::Info, t("response.link_token_issued"));
                            app.link_token = Some(token);
                            app.view = View::LinkToken;
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf("response.bad_link_token", &[("error", &err.to_string())]),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::DeviceRevoke(device_id)) => {
                    app.log(
                        StreamKind::Info,
                        tf(
                            "response.device_revoked",
                            &[("device_id", &device_id.to_string())],
                        ),
                    );
                    app.devices.retain(|device| device.device_id != device_id);
                    app.device_cursor = app.device_cursor.min(app.devices.len().saturating_sub(1));
                    Vec::new()
                }
                Some(PendingKind::ChatsList) => {
                    match serde_json::from_value::<Vec<ChatView>>(result) {
                        Ok(chats) => {
                            app.log(
                                StreamKind::Info,
                                tf(
                                    "response.chat_count",
                                    &[("count", &chats.len().to_string())],
                                ),
                            );
                            app.chats = chats;
                            app.chat_cursor = 0;
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf("response.bad_chat_list", &[("error", &err.to_string())]),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::MessagesList(chat_id)) => {
                    match serde_json::from_value::<Vec<MessageView>>(result) {
                        Ok(mut messages) => {
                            messages.sort_by_key(|message| message.created_at);
                            if app.focused_chat() == Some(chat_id) {
                                app.messages = messages;
                            }
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf("response.bad_message_list", &[("error", &err.to_string())]),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::MessageSend) => {
                    app.log(StreamKind::Info, t("response.message_queued"));
                    match app.focused_chat() {
                        Some(chat_id) => vec![messages_list_request(app, chat_id)],
                        None => Vec::new(),
                    }
                }
                Some(PendingKind::ContactCode) => {
                    match serde_json::from_value::<ContactCodeView>(result) {
                        Ok(code) => {
                            app.log(StreamKind::Info, t("response.contact_code_issued"));
                            app.contact_code = Some(code);
                            app.view = View::ContactCode;
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf("response.bad_contact_code", &[("error", &err.to_string())]),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::InvitationsList) => {
                    match serde_json::from_value::<Vec<InvitationView>>(result) {
                        Ok(invitations) => {
                            app.log(
                                StreamKind::Info,
                                tf(
                                    "response.invitation_count",
                                    &[("count", &invitations.len().to_string())],
                                ),
                            );
                            app.invitations = invitations;
                            app.invitation_cursor = 0;
                            app.view = View::Invitations;
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf(
                                "response.bad_invitation_list",
                                &[("error", &err.to_string())],
                            ),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::InvitationCreate) => {
                    match serde_json::from_value::<InvitationView>(result) {
                        Ok(invitation) => {
                            app.log(StreamKind::Info, t("response.invitation_created"));
                            app.upsert_invitation(invitation);
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf(
                                "response.bad_invitation_view",
                                &[("error", &err.to_string())],
                            ),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::InvitationAccept(invitation_id)) => {
                    match serde_json::from_value::<InvitationView>(result) {
                        Ok(invitation) => {
                            app.log(
                                StreamKind::Info,
                                tf(
                                    "response.invitation_accepted",
                                    &[("invitation_id", &invitation_id.to_string())],
                                ),
                            );
                            app.upsert_invitation(invitation);
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf(
                                "response.bad_invitation_view",
                                &[("error", &err.to_string())],
                            ),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::InvitationReject(invitation_id)) => {
                    match serde_json::from_value::<InvitationView>(result) {
                        Ok(invitation) => {
                            app.log(
                                StreamKind::Info,
                                tf(
                                    "response.invitation_rejected",
                                    &[("invitation_id", &invitation_id.to_string())],
                                ),
                            );
                            app.upsert_invitation(invitation);
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf(
                                "response.bad_invitation_view",
                                &[("error", &err.to_string())],
                            ),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::InvitationCancel(invitation_id)) => {
                    match serde_json::from_value::<InvitationView>(result) {
                        Ok(invitation) => {
                            app.log(
                                StreamKind::Info,
                                tf(
                                    "response.invitation_cancelled",
                                    &[("invitation_id", &invitation_id.to_string())],
                                ),
                            );
                            app.upsert_invitation(invitation);
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf(
                                "response.bad_invitation_view",
                                &[("error", &err.to_string())],
                            ),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::GroupCreate) => {
                    app.log(StreamKind::Info, t("response.group_created"));
                    vec![chats_list_request(app)]
                }
                Some(PendingKind::SafetyNumber(user_id)) => {
                    match serde_json::from_value::<SafetyNumberView>(result) {
                        Ok(safety_number) => {
                            app.log(
                                StreamKind::Info,
                                tf(
                                    "response.safety_number",
                                    &[("safety_number", &safety_number.safety_number)],
                                ),
                            );
                            app.safety_number = Some(safety_number);
                            ask_verify_contact(app, user_id);
                        }
                        Err(err) => app.log(
                            StreamKind::Problem,
                            tf("response.bad_safety_number", &[("error", &err.to_string())]),
                        ),
                    }
                    Vec::new()
                }
                Some(PendingKind::ContactVerify(user_id)) => {
                    let verified_at =
                        serde_json::from_value(result["verified_at"].clone()).unwrap_or(None);
                    app.log(
                        StreamKind::Info,
                        tf(
                            "response.contact_verified",
                            &[("user_id", &user_id.to_string())],
                        ),
                    );
                    if let Some(safety_number) = &mut app.safety_number
                        && safety_number.user_id == user_id
                    {
                        safety_number.verified_at = verified_at;
                    }
                    Vec::new()
                }
                None => {
                    app.log(
                        StreamKind::Problem,
                        tf("stream.unknown_response", &[("id", &id.to_string())]),
                    );
                    Vec::new()
                }
            }
        }
    }
}

fn on_session_init(app: &mut App, result: Value) -> Vec<Request> {
    match serde_json::from_value::<SessionInit>(result) {
        Ok(init) => {
            app.identity = IdentityInfo {
                state: init.identity_state,
                user_id: init.user_id,
                device_id: init.device_id,
            };
            app.authenticated = init.authenticated;
            app.connectivity = init.connectivity;

            if vault_is_locked(&init.vault_state) {
                lock(app);
                app.log(StreamKind::Info, t("unlock.locked"));
                return Vec::new();
            }

            release(app);
            what_now(app)
        }
        Err(err) => {
            app.log(
                StreamKind::Problem,
                tf("response.bad_session_init", &[("error", &err.to_string())]),
            );
            Vec::new()
        }
    }
}

fn on_identity(app: &mut App, result: Value) -> Vec<Request> {
    match serde_json::from_value::<IdentityInfo>(result) {
        Ok(identity) => {
            app.identity = identity;
            what_now(app)
        }
        Err(err) => {
            app.log(
                StreamKind::Problem,
                tf("response.bad_identity", &[("error", &err.to_string())]),
            );
            Vec::new()
        }
    }
}

fn what_now(app: &mut App) -> Vec<Request> {
    if app.identity.is_conflicted() {
        explain_conflict(app);
    } else if !app.identity.is_registered() {
        app.log(StreamKind::Info, t("boot.no_identity"));
        app.log(StreamKind::Info, t("boot.hint_register"));
        app.log(StreamKind::Info, t("boot.hint_link"));
    } else {
        app.log(
            StreamKind::Info,
            tf(
                "boot.signed_in_as",
                &[("label", &app.identity.short_label().unwrap_or_default())],
            ),
        );
        return vec![chats_list_request(app)];
    }

    Vec::new()
}

fn lock(app: &mut App) {
    app.locked = true;
    app.view = View::Unlock;
    app.input = InputMode::Pin(String::new());
    app.focus = Focus::Command;
}

fn release(app: &mut App) {
    app.locked = false;

    if app.view == View::Unlock {
        app.view = View::Stream;
        app.input = InputMode::Idle;
        app.focus = Focus::Main;
    }
}

// The response to our own `vault.unlock` and the broadcast that follows it both land here, so
// opening twice must cost nothing.
fn unlock(app: &mut App) -> Vec<Request> {
    if !app.locked {
        return Vec::new();
    }

    release(app);
    app.log(StreamKind::Info, t("unlock.unlocked"));

    vec![identity_get_request(app)]
}

fn on_error(app: &mut App, kind: Option<PendingKind>, error: RpcError) {
    if error.code == -32003 {
        app.protocol_mismatch = true;
        app.log(
            StreamKind::Problem,
            tf(
                "error.protocol_mismatch",
                &[
                    ("expected", &PROTOCOL_VERSION.to_string()),
                    ("message", &error.message),
                ],
            ),
        );
        return;
    }

    // CODE_WRONG_STATE (-32005) sluzy i cyklowi tozsamosci, i cyklowi zaproszen,
    // wiec tylko rodzaj zadania w locie pozwala je rozroznic.
    let identity_lifecycle = matches!(
        kind,
        Some(PendingKind::IdentityRegister)
            | Some(PendingKind::IdentityLink)
            | Some(PendingKind::IdentityForget)
    );

    let explanation = match error.code {
        -32005 if identity_lifecycle => tf(
            "error.wrong_identity_state",
            &[("state", &app.identity.state)],
        ),
        -32005 => t("error.wrong_state"),
        -32007 => t("error.link_rejected"),
        -32008 => t("error.no_session"),
        -32009 => t("error.locked"),
        -32010 => t("error.bad_pin"),
        -32012 => t("error.identity_mismatch"),
        -32011 => t("error.invite_rejected"),
        -32002 => t("error.not_found"),
        -32602 => t("error.bad_params"),
        -32601 => t("error.unknown_method"),
        _ => error.message.clone(),
    };

    app.log(
        StreamKind::Problem,
        tf(
            "error.line",
            &[
                ("code", &error.code.to_string()),
                ("explanation", &explanation),
                ("message", &error.message),
            ],
        ),
    );

    if matches!(kind, Some(PendingKind::MessageSend)) {
        app.log(StreamKind::Problem, t("error.send_failed"));
    }

    if error.code == -32012 {
        app.log(StreamKind::Problem, t("error.identity_mismatch_warning"));
    }
}

fn ask_verify_contact(app: &mut App, user_id: Uuid) {
    let safety_number = app
        .safety_number
        .as_ref()
        .map(|view| view.safety_number.clone())
        .unwrap_or_default();

    app.confirm = Some(Confirm {
        prompt: tf(
            "confirm.verify_contact",
            &[
                ("user_id", &user_id.to_string()),
                ("safety_number", &safety_number),
            ],
        ),
        kind: ConfirmKind::VerifyContact(user_id),
    });
}

fn bad_payload(method: &str, error: impl std::fmt::Display) -> String {
    tf(
        "notify.bad_payload",
        &[("method", method), ("error", &error.to_string())],
    )
}

fn explain_conflict(app: &mut App) {
    app.log(StreamKind::Problem, t("conflict.explain"));
    app.log(StreamKind::Info, t("conflict.hint"));
}

pub fn identity_get_request(app: &mut App) -> Request {
    let id = app.track(PendingKind::IdentityGet);
    Request {
        id,
        method: "identity.get".to_string(),
        params: serde_json::json!({}),
    }
}

pub fn chats_list_request(app: &mut App) -> Request {
    let id = app.track(PendingKind::ChatsList);
    Request {
        id,
        method: "chats.list".to_string(),
        params: serde_json::json!({
            "filter": {
                "page": 0,
                "per_page": 50,
                "search_in_latest": true,
                "is_active": true
            },
            "pagination": {
                "currentPage": 0,
                "perPage": 50,
                "totalPages": 1,
                "totalCount": 50
            }
        }),
    }
}

pub fn messages_list_request(app: &mut App, chat_id: Uuid) -> Request {
    let id = app.track(PendingKind::MessagesList(chat_id));
    Request {
        id,
        method: "messages.list".to_string(),
        params: serde_json::json!({
            "filter": {
                "page": 0,
                "per_page": 50,
                "chat_id": chat_id,
                "search_in_latest": true
            },
            "pagination": {
                "currentPage": 0,
                "perPage": 50,
                "totalPages": 1,
                "totalCount": 50
            }
        }),
    }
}
