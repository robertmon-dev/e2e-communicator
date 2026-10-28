use std::sync::OnceLock;

pub const MISSING_OPEN: &str = "⟦";
pub const MISSING_CLOSE: &str = "⟧";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
}

impl Locale {
    pub const DEFAULT: Self = Self::En;

    pub fn parse(tag: &str) -> Option<Self> {
        let language = tag
            .split(['.', '_', '-'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();

        match language.as_str() {
            "en" | "c" | "posix" => Some(Self::En),
            _ => None,
        }
    }

    pub fn from_env() -> Self {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .filter(|value| !value.is_empty())
            .find_map(|value| Self::parse(&value))
            .unwrap_or(Self::DEFAULT)
    }

    pub fn catalogue(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::En => EN,
        }
    }
}

static ACTIVE: OnceLock<Locale> = OnceLock::new();

pub fn set_locale(locale: Locale) {
    let _ = ACTIVE.set(locale);
}

pub fn locale() -> Locale {
    *ACTIVE.get().unwrap_or(&Locale::DEFAULT)
}

pub fn t(key: &str) -> String {
    strip(&raw(key, &[]))
}

pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    strip(&raw(key, args))
}

pub const TOKEN_OPEN: &str = "[[";
pub const TOKEN_CLOSE: &str = "]]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Plain(String),
    Token(String),
}

pub fn segments(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut rest = text;

    while let Some(open) = rest.find(TOKEN_OPEN) {
        let after = &rest[open + TOKEN_OPEN.len()..];
        let Some(close) = after.find(TOKEN_CLOSE) else {
            break;
        };

        if open > 0 {
            out.push(Segment::Plain(rest[..open].to_string()));
        }
        out.push(Segment::Token(after[..close].to_string()));
        rest = &after[close + TOKEN_CLOSE.len()..];
    }

    if !rest.is_empty() {
        out.push(Segment::Plain(rest.to_string()));
    }

    out
}

pub fn segments_of(key: &str, args: &[(&str, &str)]) -> Vec<Segment> {
    segments(&raw(key, args))
}

fn strip(text: &str) -> String {
    text.replace(TOKEN_OPEN, "").replace(TOKEN_CLOSE, "")
}

fn raw(key: &str, args: &[(&str, &str)]) -> String {
    let Some(template) = template(key) else {
        return missing(key);
    };

    let mut text = template.to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

pub fn template(key: &str) -> Option<&'static str> {
    locale()
        .catalogue()
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, text)| *text)
}

fn missing(key: &str) -> String {
    format!("{MISSING_OPEN}{key}{MISSING_CLOSE}")
}

pub const EN: &[(&str, &str)] = &[
    ("app.queue_full", "request queue full, try again"),
    (
        "app.shutdown_timed_out",
        "tui: the client connection did not close within {grace}",
    ),
    ("app.shutting_down", "shutting down…"),
    (
        "boot.hint_link",
        "  :link <user_id> <token> — join an account",
    ),
    (
        "boot.hint_register",
        "  :register               — a new account",
    ),
    ("boot.no_identity", "this device has no identity"),
    ("boot.signed_in_as", "signed in as {label}"),
    ("command.bad_chat_id", "'{value}' is not a valid chat_id"),
    (
        "command.bad_device_id",
        "'{value}' is not a valid device_id",
    ),
    (
        "command.bad_invitation_id",
        "'{value}' is not a valid invitation_id",
    ),
    ("command.bad_user_id", "'{value}' is not a valid user_id"),
    ("command.cancelled", "cancelled"),
    ("command.empty", "empty command"),
    ("command.no_open_chat", "no chat is open"),
    ("command.stream_raw", "stream: raw"),
    ("command.stream_readable", "stream: readable"),
    ("command.unknown", "unknown command: {name}"),
    ("command.usage_accept", "usage: :accept <invitation_id>"),
    ("command.usage_cancel", "usage: :cancel <invitation_id>"),
    ("command.usage_group", "usage: :group <name>"),
    (
        "command.usage_invite",
        "usage: :invite <code> <label> [chat_id]",
    ),
    (
        "command.usage_link",
        "usage: :link <user_id> <token> <user_seed>",
    ),
    ("command.usage_reject", "usage: :reject <invitation_id>"),
    ("command.usage_revoke", "usage: :revoke <device_id>"),
    ("command.usage_verify", "usage: :verify <user_id>"),
    (
        "confirm.forget",
        "Forget the local identity? Chats stay; the session and keys do not. [y/n]",
    ),
    (
        "confirm.reject_invitation",
        "Reject invitation {invitation_id}? [y/n]",
    ),
    ("confirm.revoke_other", "Revoke device {device_id}? [y/n]"),
    (
        "confirm.revoke_own",
        "Revoke THIS device ({device_id})? You will be signed out and lose the identity. [y/n]",
    ),
    (
        "confirm.verify_contact",
        "safety number: {safety_number}\n\nDoes it match what {user_id} shows on their device? [y/n]",
    ),
    (
        "conflict.explain",
        "identity in conflict: the relay rejected it, so either this device was revoked or the user_id is taken",
    ),
    (
        "conflict.hint",
        "  :forget — drop it locally, then :register",
    ),
    ("conn.app_stopped_listening", "the app stopped listening"),
    ("conn.app_stopped_sending", "the app stopped sending"),
    ("conn.closed_by_client", "the client closed the socket"),
    ("conn.excerpt_more", "… (+{bytes} bytes)"),
    ("conn.frame_too_large", "a frame exceeded {limit} bytes"),
    ("conn.not_connected_yet", "not connected yet"),
    ("conn.shutting_down", "shutting down"),
    ("error.bad_params", "bad parameters"),
    ("error.bad_pin", "the PIN was refused"),
    (
        "error.identity_mismatch",
        "the peer's identity does not match what was pinned",
    ),
    (
        "error.identity_mismatch_warning",
        "WARNING: this looks like an identity-pinning attack, do not trust this chat until the contact is re-verified",
    ),
    (
        "error.invite_rejected",
        "the invitation was rejected, it is either a duplicate or not allowed",
    ),
    ("error.line", "error {code}: {explanation} ({message})"),
    (
        "error.link_rejected",
        "the relay rejected the link, so the token is unknown, used or expired",
    ),
    (
        "error.locked",
        "the vault is shut, so the client refused this",
    ),
    (
        "error.no_session",
        "no relay session yet, the authenticator is still signing in",
    ),
    ("error.not_found", "not found"),
    (
        "error.protocol_mismatch",
        "protocol version mismatch, this build speaks {expected}: {message}",
    ),
    (
        "error.send_failed",
        "the send failed, most likely because the chat has no keys yet",
    ),
    (
        "error.unknown_method",
        "the client does not know this method",
    ),
    (
        "error.wrong_identity_state",
        "wrong identity state, currently {state}",
    ),
    ("error.wrong_state", "wrong state for this request"),
    ("help.accept", "accept an incoming invitation"),
    ("help.cancel", "cancel an invitation you sent"),
    ("help.chats", "list the chats"),
    ("help.clear", "empty the stream"),
    (
        "help.code",
        "show this account's contact code as a [[QR]] code",
    ),
    ("help.debug", "toggle the raw [[IPC]] frame view"),
    ("help.devices", "the devices on this account"),
    ("help.forget", "forget the local identity"),
    ("help.group", "start a group chat"),
    ("help.help", "this list"),
    (
        "help.invitations",
        "list invitations sent to and received from contacts",
    ),
    (
        "help.invite",
        "invite a contact by their code, starting a chat",
    ),
    (
        "help.keys_chat",
        "in a chat you simply type; [[:]] starts a command, [[::]] a message that begins with a colon",
    ),
    (
        "help.keys_command",
        "[[:]] opens the command line, [[esc]] or [[backspace]] closes it",
    ),
    (
        "help.keys_devices",
        "in the device list: [[l]] for a link token, [[r]] to revoke the selected device",
    ),
    (
        "help.keys_focus",
        "[[alt]] + arrows move focus between the panes",
    ),
    (
        "help.keys_help",
        "[[alt]] + [[h]] docks this list on the left, [[:help]] centres it",
    ),
    (
        "help.keys_lists",
        "[[↑]] [[↓]] to move, [[enter]] to open a chat, [[esc]] to go back",
    ),
    ("help.keys_title", "keys:"),
    (
        "help.keys_unlock",
        "while the vault is shut: type the [[PIN]], [[enter]] opens it",
    ),
    (
        "help.link",
        "join this device to an account that already exists",
    ),
    ("help.link_token", "show a link token for another device"),
    ("help.quit", "leave the app, also [[:exit]] and [[:q]]"),
    ("help.register", "mint a new identity on this device"),
    ("help.reject", "reject an incoming invitation"),
    ("help.revoke", "revoke a device"),
    (
        "help.verify",
        "fetch a contact's safety number and verify it",
    ),
    ("notify.bad_payload", "malformed {method}: {error}"),
    ("notify.chat_activated", "a chat became active"),
    ("notify.chat_member_added", "a member was added to a chat"),
    ("notify.connectivity", "connectivity: {status}"),
    ("notify.identity", "identity: {state}"),
    (
        "notify.invitation_received",
        "invitation received: {invitation_id}",
    ),
    (
        "notify.invitation_resolved",
        "invitation resolved: {invitation_id}",
    ),
    (
        "notify.key_exchange_progress",
        "key exchange: {known}/{total}",
    ),
    ("notify.message_delivered", "message delivered"),
    (
        "notify.message_elsewhere",
        "a new message in chat {chat_id}",
    ),
    ("notify.session_none", "session: none"),
    ("notify.session_on", "session: signed in"),
    ("response.bad_chat_list", "malformed chat list: {error}"),
    (
        "response.bad_contact_code",
        "malformed contact code: {error}",
    ),
    ("response.bad_device_list", "malformed device list: {error}"),
    ("response.bad_identity", "malformed identity: {error}"),
    (
        "response.bad_invitation_list",
        "malformed invitation list: {error}",
    ),
    (
        "response.bad_invitation_view",
        "malformed invitation: {error}",
    ),
    ("response.bad_link_token", "malformed link token: {error}"),
    (
        "response.bad_message_list",
        "malformed message list: {error}",
    ),
    (
        "response.bad_safety_number",
        "malformed safety number: {error}",
    ),
    (
        "response.bad_session_init",
        "malformed session.init: {error}",
    ),
    ("response.chat_count", "{count} chats"),
    ("response.contact_code_issued", "contact code issued"),
    (
        "response.contact_verified",
        "contact {user_id} marked verified",
    ),
    ("response.device_count", "{count} devices"),
    ("response.device_revoked", "device {device_id} revoked"),
    (
        "response.forgotten",
        "identity forgotten, you can register a new one",
    ),
    ("response.group_created", "group chat created"),
    (
        "response.invitation_accepted",
        "invitation {invitation_id} accepted",
    ),
    (
        "response.invitation_cancelled",
        "invitation {invitation_id} cancelled",
    ),
    ("response.invitation_count", "{count} invitations"),
    ("response.invitation_created", "invitation sent"),
    (
        "response.invitation_rejected",
        "invitation {invitation_id} rejected",
    ),
    ("response.link_token_issued", "link token issued"),
    (
        "response.linked",
        "this device is now linked to the account",
    ),
    ("response.message_queued", "message stored and queued"),
    ("response.registered", "registered a new identity"),
    ("response.safety_number", "safety number: {safety_number}"),
    ("status.connectivity_unknown", "unknown"),
    (
        "status.keys_conflict",
        "  [[:forget]], then [[:register]]  [[:help]]",
    ),
    ("status.keys_conversation", "  [[esc]] → list"),
    (
        "status.keys_default",
        "  [[:chats]]  [[:devices]]  [[:link-token]]  [[:help]]",
    ),
    ("status.keys_devices", "  [[l]] token  [[r]] revoke"),
    ("status.keys_link_token", "  [[esc]] → list  [[:devices]]"),
    ("status.keys_unlock", "  type the [[PIN]]  [[:quit]]"),
    (
        "status.keys_unregistered",
        "  [[:register]]  [[:link]]  [[:help]]",
    ),
    ("status.no_client", "no client ({reason})"),
    ("status.no_identity", "no identity"),
    ("status.session_off", "○ session"),
    ("status.session_on", "● session"),
    ("stream.connected", "connected to the client"),
    ("stream.disconnected", "no client: {reason}"),
    ("stream.malformed_frame", "unreadable frame: {line}"),
    ("stream.title", " e2e-communicator "),
    ("stream.title_raw", " e2e-communicator — raw "),
    (
        "stream.unknown_notification",
        "unknown notification: {method}",
    ),
    (
        "stream.unknown_response",
        "a response to request #{id}, which nothing is waiting for",
    ),
    ("ui.chat_fallback_title", "chat {id}"),
    (
        "ui.chats_empty",
        "No chats.\n\nStarting one needs the invitation protocol, which is not built yet.",
    ),
    (
        "ui.chats_no_identity",
        "No identity.\n\nStart with [[:register]] or [[:link]].",
    ),
    ("ui.chats_title", " chats "),
    ("ui.command_title", " command "),
    ("ui.confirm_title", " confirm "),
    ("ui.contact_code_label_code", "code     "),
    ("ui.contact_code_label_user_id", "user_id  "),
    (
        "ui.contact_code_missing",
        "No contact code. [[:code]] issues one.",
    ),
    ("ui.contact_code_title", " contact code "),
    ("ui.conversation_empty", "No messages in this chat."),
    ("ui.conversation_me", "me"),
    (
        "ui.devices_empty",
        "No devices. [[:devices]] refreshes the list.",
    ),
    ("ui.devices_entry", "{id}  reg. {time}{marker}"),
    (
        "ui.devices_key_link",
        "[[l]]  show the link token as a [[QR]] code",
    ),
    ("ui.devices_key_revoke", "[[r]]  revoke the selected device"),
    ("ui.devices_this_one", "  this device"),
    ("ui.devices_title", " devices "),
    ("ui.help_title", " commands "),
    ("ui.invitation_age", "{minutes}m ago"),
    ("ui.invitation_direction_in", "in"),
    ("ui.invitation_direction_out", "out"),
    ("ui.invitation_status_accepted", "accepted"),
    ("ui.invitation_status_cancelled", "cancelled"),
    ("ui.invitation_status_pending", "pending"),
    ("ui.invitation_status_rejected", "rejected"),
    (
        "ui.invitations_empty",
        "No invitations. [[:invitations]] refreshes the list.",
    ),
    (
        "ui.invitations_entry",
        "{direction}  {counterpart}  {status}  {age}",
    ),
    ("ui.invitations_title", " invitations "),
    ("ui.key_exchange_progress", "(keys {known}/{total})"),
    (
        "ui.locked_body",
        "The client's database is shut.\n\nType the 8-digit [[PIN]] below, then press [[enter]].\n\nNothing else works until it opens, and [[:quit]] leaves.",
    ),
    ("ui.locked_title", " locked "),
    ("ui.message_title", " message "),
    ("ui.qr_build_failed", "could not build the QR code: {error}"),
    (
        "ui.qr_expired",
        "expired — [[:link-token]] issues a new one",
    ),
    ("ui.qr_expires_in", "expires in {minutes}:{seconds}"),
    ("ui.qr_label_token", "token    "),
    ("ui.qr_label_user_id", "user_id  "),
    ("ui.qr_no_token", "No token. [[:link-token]] issues one."),
    (
        "ui.qr_other_device",
        "on the other device: [[:link <user_id> <token>]]",
    ),
    ("ui.qr_title", " link token "),
    ("ui.qr_too_narrow", "pane too narrow"),
    (
        "ui.qr_too_short",
        "pane too short for the QR code, {missing} lines short",
    ),
    ("ui.unlock_title", " unlock "),
    ("unlock.bad_format", "the PIN is exactly 8 digits"),
    ("unlock.locked", "the client is shut, waiting for the PIN"),
    (
        "unlock.needed",
        "the vault is shut: open it with the PIN first",
    ),
    ("unlock.relocked", "the client shut itself again"),
    ("unlock.unlocked", "vault open"),
];
