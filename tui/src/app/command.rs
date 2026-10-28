use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use uuid::Uuid;

use crate::app::state::{
    App, Confirm, ConfirmKind, Focus, HelpPlacement, InputMode, PendingKind, StreamKind, View,
};
use crate::app::update::{chats_list_request, messages_list_request};
use crate::i18n::{t, tf};
use crate::ipc::protocol::{Request, VAULT_UNLOCK_METHOD};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Register,
    Link {
        user_id: Uuid,
        token: String,
        user_seed: String,
    },
    Forget,
    Devices,
    LinkToken,
    Revoke {
        device_id: Uuid,
    },
    Chats,
    Clear,
    Debug,
    Help,
    Quit,
    ContactCode,
    Invitations,
    Invite {
        code: String,
        label: String,
        chat_id: Option<Uuid>,
    },
    Accept(Uuid),
    Reject(Uuid),
    Cancel(Uuid),
    Group(String),
    Verify(Uuid),
}

pub const HELP: &[(&str, &str)] = &[
    (":help", "help.help"),
    (":register", "help.register"),
    (":link <user_id> <token> <user_seed>", "help.link"),
    (":link-token", "help.link_token"),
    (":devices", "help.devices"),
    (":revoke <device_id>", "help.revoke"),
    (":forget", "help.forget"),
    (":chats", "help.chats"),
    (":code", "help.code"),
    (":invite <code> <label> [chat_id]", "help.invite"),
    (":invitations", "help.invitations"),
    (":accept <id>", "help.accept"),
    (":reject <id>", "help.reject"),
    (":cancel <id>", "help.cancel"),
    (":group <name>", "help.group"),
    (":verify <user_id>", "help.verify"),
    (":clear", "help.clear"),
    (":debug", "help.debug"),
    (":quit", "help.quit"),
];

pub const PIN_LENGTH: usize = 8;

pub const HELP_KEYS: &[&str] = &[
    "help.keys_help",
    "help.keys_command",
    "help.keys_focus",
    "help.keys_lists",
    "help.keys_devices",
    "help.keys_chat",
    "help.keys_unlock",
];

pub fn parse(line: &str) -> Result<Command, String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix(':').unwrap_or(trimmed).trim();
    if trimmed.is_empty() {
        return Err(t("command.empty"));
    }

    let mut parts = trimmed.split_whitespace();
    let name = parts.next().unwrap_or_default().to_lowercase();
    let rest: Vec<&str> = parts.collect();

    match name.as_str() {
        "register" => Ok(Command::Register),
        "forget" => Ok(Command::Forget),
        "devices" => Ok(Command::Devices),
        "link-token" => Ok(Command::LinkToken),
        "chats" => Ok(Command::Chats),
        "clear" | "cls" => Ok(Command::Clear),
        "debug" => Ok(Command::Debug),
        "help" | "h" | "?" => Ok(Command::Help),
        "quit" | "exit" | "q" => Ok(Command::Quit),
        "link" => match rest.as_slice() {
            [user_id, token, user_seed] => Ok(Command::Link {
                user_id: Uuid::parse_str(user_id)
                    .map_err(|_| tf("command.bad_user_id", &[("value", user_id)]))?,
                token: (*token).to_string(),
                user_seed: (*user_seed).to_string(),
            }),
            _ => Err(t("command.usage_link")),
        },
        "revoke" => match rest.as_slice() {
            [device_id] => Ok(Command::Revoke {
                device_id: Uuid::parse_str(device_id)
                    .map_err(|_| tf("command.bad_device_id", &[("value", device_id)]))?,
            }),
            _ => Err(t("command.usage_revoke")),
        },
        "code" => Ok(Command::ContactCode),
        "invitations" => Ok(Command::Invitations),
        "invite" => match rest.as_slice() {
            [code, label] => Ok(Command::Invite {
                code: (*code).to_string(),
                label: (*label).to_string(),
                chat_id: None,
            }),
            [code, label, chat_id] => Ok(Command::Invite {
                code: (*code).to_string(),
                label: (*label).to_string(),
                chat_id: Some(
                    Uuid::parse_str(chat_id)
                        .map_err(|_| tf("command.bad_chat_id", &[("value", chat_id)]))?,
                ),
            }),
            _ => Err(t("command.usage_invite")),
        },
        "accept" => match rest.as_slice() {
            [invitation_id] => Ok(Command::Accept(
                Uuid::parse_str(invitation_id)
                    .map_err(|_| tf("command.bad_invitation_id", &[("value", invitation_id)]))?,
            )),
            _ => Err(t("command.usage_accept")),
        },
        "reject" => match rest.as_slice() {
            [invitation_id] => Ok(Command::Reject(
                Uuid::parse_str(invitation_id)
                    .map_err(|_| tf("command.bad_invitation_id", &[("value", invitation_id)]))?,
            )),
            _ => Err(t("command.usage_reject")),
        },
        "cancel" => match rest.as_slice() {
            [invitation_id] => Ok(Command::Cancel(
                Uuid::parse_str(invitation_id)
                    .map_err(|_| tf("command.bad_invitation_id", &[("value", invitation_id)]))?,
            )),
            _ => Err(t("command.usage_cancel")),
        },
        "group" => {
            if rest.is_empty() {
                Err(t("command.usage_group"))
            } else {
                Ok(Command::Group(rest.join(" ")))
            }
        }
        "verify" => match rest.as_slice() {
            [user_id] => {
                Ok(Command::Verify(Uuid::parse_str(user_id).map_err(|_| {
                    tf("command.bad_user_id", &[("value", user_id)])
                })?))
            }
            _ => Err(t("command.usage_verify")),
        },
        other => Err(tf("command.unknown", &[("name", other)])),
    }
}

pub fn on_key(app: &mut App, key: KeyEvent) -> Vec<Request> {
    if !matches!(key.kind, KeyEventKind::Press) {
        return Vec::new();
    }

    if app.confirm.is_some() {
        return on_confirm_key(app, key);
    }

    if key.modifiers.contains(KeyModifiers::ALT) {
        return on_alt_key(app, key.code);
    }

    match key.code {
        KeyCode::Esc => {
            if app.help.take().is_some() {
                return Vec::new();
            }

            app.view = home_view(app);
            app.focus = home_focus(app);
            app.input = idle_input(app);
            Vec::new()
        }

        KeyCode::Enter => on_enter(app),
        KeyCode::Backspace => {
            match &mut app.input {
                InputMode::Idle => {}
                InputMode::Command(buffer)
                | InputMode::Message(buffer)
                | InputMode::Pin(buffer) => {
                    buffer.pop();
                }
            }
            if matches!(&app.input, InputMode::Command(buffer) if buffer.is_empty()) {
                app.input = idle_input(app);
                app.focus = home_focus(app);
            }
            Vec::new()
        }
        KeyCode::Up => {
            move_cursor(app, false);
            Vec::new()
        }
        KeyCode::Down => {
            move_cursor(app, true);
            Vec::new()
        }
        KeyCode::Tab => Vec::new(),
        KeyCode::Char(character) => on_char(app, character),
        _ => Vec::new(),
    }
}

fn idle_input(app: &App) -> InputMode {
    if app.locked {
        return InputMode::Pin(String::new());
    }

    match app.focused_chat() {
        Some(_) => InputMode::Message(String::new()),
        None => InputMode::Idle,
    }
}

fn home_view(app: &App) -> View {
    if app.locked {
        View::Unlock
    } else {
        View::Stream
    }
}

fn home_focus(app: &App) -> Focus {
    if app.locked {
        Focus::Command
    } else {
        Focus::Main
    }
}

fn on_alt_key(app: &mut App, code: KeyCode) -> Vec<Request> {
    if let KeyCode::Char(character) = code
        && character.eq_ignore_ascii_case(&'h')
    {
        app.help = match app.help {
            Some(HelpPlacement::Left) => None,
            _ => Some(HelpPlacement::Left),
        };
        return Vec::new();
    }

    let forward = match code {
        KeyCode::Right | KeyCode::Down => true,
        KeyCode::Left | KeyCode::Up => false,
        _ => return Vec::new(),
    };

    set_focus(app, app.focus.step(forward));
    Vec::new()
}

fn set_focus(app: &mut App, focus: Focus) {
    app.focus = focus;

    match focus {
        Focus::Command => {
            if app.input == InputMode::Idle {
                app.input = InputMode::Command(String::new());
            }
        }
        _ => {
            if matches!(&app.input, InputMode::Command(buffer) if buffer.is_empty()) {
                app.input = idle_input(app);
            }
        }
    }
}

fn on_char(app: &mut App, character: char) -> Vec<Request> {
    let in_chat = app.focused_chat().is_some();

    match &mut app.input {
        InputMode::Message(buffer) => {
            if character == ':' && buffer.is_empty() {
                app.input = InputMode::Command(String::new());
            } else {
                buffer.push(character);
            }
            Vec::new()
        }
        InputMode::Command(buffer) => {
            if character == ':' && buffer.is_empty() && in_chat {
                app.input = InputMode::Message(":".to_string());
                return Vec::new();
            }
            buffer.push(character);
            Vec::new()
        }
        InputMode::Pin(buffer) => {
            if character == ':' && buffer.is_empty() {
                app.input = InputMode::Command(String::new());
                app.focus = Focus::Command;
            } else if character.is_ascii_digit() && buffer.chars().count() < PIN_LENGTH {
                buffer.push(character);
            }
            Vec::new()
        }
        InputMode::Idle => {
            if character == ':' {
                app.input = InputMode::Command(String::new());
                app.focus = Focus::Command;
                return Vec::new();
            }

            match (app.view, character) {
                (View::Devices, 'l') => vec![link_token_request(app)],
                (View::Devices, 'r') => {
                    let selected = app
                        .devices
                        .get(app.device_cursor)
                        .map(|device| device.device_id);
                    if let Some(device_id) = selected {
                        ask_revoke(app, device_id);
                    }
                    Vec::new()
                }
                _ => Vec::new(),
            }
        }
    }
}

fn on_enter(app: &mut App) -> Vec<Request> {
    let next = idle_input(app);

    match std::mem::replace(&mut app.input, next) {
        InputMode::Idle => open_selected_chat(app),
        InputMode::Message(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                app.input = InputMode::Message(String::new());
                return Vec::new();
            }

            let Some(chat_id) = app.focused_chat() else {
                app.log(StreamKind::Problem, t("command.no_open_chat"));
                return Vec::new();
            };

            app.input = InputMode::Message(String::new());
            let id = app.track(PendingKind::MessageSend);
            vec![Request {
                id,
                method: "messages.send".to_string(),
                params: serde_json::json!({ "chat_id": chat_id, "text": trimmed }),
            }]
        }
        InputMode::Pin(pin) => {
            if pin.is_empty() {
                return Vec::new();
            }

            if pin.chars().count() != PIN_LENGTH {
                app.log(StreamKind::Problem, t("unlock.bad_format"));
                return Vec::new();
            }

            let id = app.track(PendingKind::VaultUnlock);
            vec![Request {
                id,
                method: VAULT_UNLOCK_METHOD.to_string(),
                params: serde_json::json!({ "pin": pin }),
            }]
        }
        InputMode::Command(line) => {
            if line.trim().is_empty() {
                return open_selected_chat(app);
            }

            let typed = line.trim();
            let typed = typed.strip_prefix(':').unwrap_or(typed).trim();
            app.log(StreamKind::Outgoing, format!(":{typed}"));

            match parse(&line) {
                Ok(command) => run(app, command),
                Err(err) => {
                    app.log(StreamKind::Problem, err);
                    Vec::new()
                }
            }
        }
    }
}

fn open_selected_chat(app: &mut App) -> Vec<Request> {
    match app.chats.get(app.chat_cursor).map(|chat| chat.id) {
        Some(chat_id) => {
            app.messages.clear();
            app.view = View::Conversation(chat_id);
            app.input = InputMode::Message(String::new());
            vec![messages_list_request(app, chat_id)]
        }
        None => Vec::new(),
    }
}

fn needs_the_vault(command: &Command) -> bool {
    !matches!(
        command,
        Command::Clear | Command::Debug | Command::Help | Command::Quit
    )
}

fn run(app: &mut App, command: Command) -> Vec<Request> {
    if app.locked && needs_the_vault(&command) {
        app.log(StreamKind::Problem, t("unlock.needed"));
        return Vec::new();
    }

    match command {
        Command::Register => {
            let id = app.track(PendingKind::IdentityRegister);
            vec![plain(id, "identity.register")]
        }
        Command::Link {
            user_id,
            token,
            user_seed,
        } => {
            let id = app.track(PendingKind::IdentityLink);
            vec![Request {
                id,
                method: "identity.link".to_string(),
                params: serde_json::json!({
                    "user_id": user_id,
                    "token": token,
                    "user_seed": user_seed,
                }),
            }]
        }
        Command::Forget => {
            app.confirm = Some(Confirm {
                prompt: t("confirm.forget"),
                kind: ConfirmKind::ForgetIdentity,
            });
            Vec::new()
        }
        Command::Devices => {
            let id = app.track(PendingKind::DevicesList);
            vec![plain(id, "devices.list")]
        }
        Command::LinkToken => vec![link_token_request(app)],
        Command::Revoke { device_id } => {
            ask_revoke(app, device_id);
            Vec::new()
        }
        Command::Chats => vec![chats_list_request(app)],
        Command::Clear => {
            app.stream.clear();
            app.view = home_view(app);
            Vec::new()
        }
        Command::Debug => {
            app.raw_stream = !app.raw_stream;
            app.log(
                StreamKind::Info,
                if app.raw_stream {
                    t("command.stream_raw")
                } else {
                    t("command.stream_readable")
                },
            );
            app.view = home_view(app);
            Vec::new()
        }
        Command::Help => {
            app.help = Some(HelpPlacement::Centre);
            Vec::new()
        }
        Command::Quit => {
            app.should_quit = true;
            Vec::new()
        }
        Command::ContactCode => {
            let id = app.track(PendingKind::ContactCode);
            vec![plain(id, "invitations.contact_code")]
        }
        Command::Invitations => {
            let id = app.track(PendingKind::InvitationsList);
            vec![plain(id, "invitations.list")]
        }
        Command::Invite {
            code,
            label,
            chat_id,
        } => {
            let id = app.track(PendingKind::InvitationCreate);
            vec![Request {
                id,
                method: "invitations.create".to_string(),
                params: serde_json::json!({
                    "code": code,
                    "label": label,
                    "chat_id": chat_id,
                }),
            }]
        }
        Command::Accept(invitation_id) => {
            let id = app.track(PendingKind::InvitationAccept(invitation_id));
            vec![Request {
                id,
                method: "invitations.accept".to_string(),
                params: serde_json::json!({ "invitation_id": invitation_id }),
            }]
        }
        Command::Reject(invitation_id) => {
            ask_reject_invitation(app, invitation_id);
            Vec::new()
        }
        Command::Cancel(invitation_id) => {
            let id = app.track(PendingKind::InvitationCancel(invitation_id));
            vec![Request {
                id,
                method: "invitations.cancel".to_string(),
                params: serde_json::json!({ "invitation_id": invitation_id }),
            }]
        }
        Command::Group(name) => {
            let id = app.track(PendingKind::GroupCreate);
            vec![Request {
                id,
                method: "chats.create_group".to_string(),
                params: serde_json::json!({ "name": name }),
            }]
        }
        Command::Verify(user_id) => {
            let id = app.track(PendingKind::SafetyNumber(user_id));
            vec![Request {
                id,
                method: "users.safety_number".to_string(),
                params: serde_json::json!({ "user_id": user_id }),
            }]
        }
    }
}

fn ask_revoke(app: &mut App, device_id: Uuid) {
    let own = app.identity.device_id == Some(device_id);
    let key = if own {
        "confirm.revoke_own"
    } else {
        "confirm.revoke_other"
    };
    let prompt = tf(key, &[("device_id", &device_id.to_string())]);

    app.confirm = Some(Confirm {
        prompt,
        kind: ConfirmKind::RevokeDevice(device_id),
    });
}

fn ask_reject_invitation(app: &mut App, invitation_id: Uuid) {
    let prompt = tf(
        "confirm.reject_invitation",
        &[("invitation_id", &invitation_id.to_string())],
    );

    app.confirm = Some(Confirm {
        prompt,
        kind: ConfirmKind::RejectInvitation(invitation_id),
    });
}

fn on_confirm_key(app: &mut App, key: KeyEvent) -> Vec<Request> {
    let Some(confirm) = app.confirm.take() else {
        return Vec::new();
    };

    let unmodified = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;

    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') if unmodified => match confirm.kind {
            ConfirmKind::RevokeDevice(device_id) => {
                let id = app.track(PendingKind::DeviceRevoke(device_id));
                vec![Request {
                    id,
                    method: "devices.revoke".to_string(),
                    params: serde_json::json!({ "device_id": device_id }),
                }]
            }
            ConfirmKind::ForgetIdentity => {
                let id = app.track(PendingKind::IdentityForget);
                vec![plain(id, "identity.forget")]
            }
            ConfirmKind::RejectInvitation(invitation_id) => {
                let id = app.track(PendingKind::InvitationReject(invitation_id));
                vec![Request {
                    id,
                    method: "invitations.reject".to_string(),
                    params: serde_json::json!({ "invitation_id": invitation_id }),
                }]
            }
            ConfirmKind::VerifyContact(user_id) => {
                let id = app.track(PendingKind::ContactVerify(user_id));
                vec![Request {
                    id,
                    method: "users.verify".to_string(),
                    params: serde_json::json!({ "user_id": user_id }),
                }]
            }
        },
        _ => {
            app.log(StreamKind::Info, t("command.cancelled"));
            Vec::new()
        }
    }
}

fn link_token_request(app: &mut App) -> Request {
    let id = app.track(PendingKind::LinkToken);
    plain(id, "devices.create_link_token")
}

fn plain(id: u64, method: &str) -> Request {
    Request {
        id,
        method: method.to_string(),
        params: serde_json::json!({}),
    }
}

fn move_cursor(app: &mut App, forward: bool) {
    let (cursor, len) = match (app.focus, app.view) {
        (Focus::Command, _) => return,
        (Focus::Chats, _) => (&mut app.chat_cursor, app.chats.len()),
        (Focus::Main, View::Devices) => (&mut app.device_cursor, app.devices.len()),
        (Focus::Main, View::Invitations) => (&mut app.invitation_cursor, app.invitations.len()),
        (Focus::Main, _) => (&mut app.chat_cursor, app.chats.len()),
    };

    if len == 0 {
        *cursor = 0;
    } else if forward {
        *cursor = (*cursor + 1).min(len - 1);
    } else {
        *cursor = cursor.saturating_sub(1);
    }
}
