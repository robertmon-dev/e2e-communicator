use chrono::{Duration, Utc};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use uuid::Uuid;

use ratatui::style::Color;
use tui::app::state::{App, ConnectionStatus, StreamKind, View};

use tui::ipc::protocol::{DeviceKeysView, DeviceView, IdentityInfo, LinkTokenView};
use tui::ui::theme;

mod common;
use common::rows;

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

fn screen(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| tui::ui::draw(frame, app)).unwrap();
    rows(&terminal).join("\n")
}

fn registered_app() -> App {
    let mut app = App::new();
    app.connection = ConnectionStatus::Connected;
    app.identity = IdentityInfo {
        state: "registered".to_string(),
        user_id: Some(Uuid::new_v4()),
        device_id: Some(Uuid::new_v4()),
    };
    app.authenticated = true;
    app.connectivity = "Online".to_string();
    app
}

fn device(device_id: Uuid) -> DeviceView {
    DeviceView {
        device_id,
        keys: DeviceKeysView {
            signing_key: "a".repeat(43),
            agreement_key: "b".repeat(43),
        },
        registered_at: Utc::now(),
    }
}

#[test]
fn the_status_bar_shows_connectivity_session_and_identity() {
    let app = registered_app();

    let text = screen(&app, 80, 20);

    assert!(text.contains("Online"), "connectivity missing:\n{text}");
    assert!(
        text.contains(&text_of("status.session_on")),
        "session indicator missing:\n{text}"
    );
    assert!(
        text.contains(&app.identity.short_label().unwrap()),
        "identity label missing:\n{text}"
    );
}

#[test]
fn a_disconnected_client_is_called_out_in_the_status_bar() {
    let mut app = App::new();
    app.connection = ConnectionStatus::Disconnected {
        reason: "brak pliku".to_string(),
    };

    let text = screen(&app, 80, 20);

    assert!(
        text.contains(&prefix("status.no_client")),
        "a missing client must be obvious:\n{text}"
    );
}

#[test]
fn the_stream_view_shows_its_lines_newest_last() {
    let mut app = registered_app();
    app.log(StreamKind::Info, "pierwsza linia");
    app.log(StreamKind::Problem, "druga linia");

    let text = screen(&app, 80, 20);

    let first = text.find("pierwsza linia").expect("first line missing");
    let second = text.find("druga linia").expect("second line missing");
    assert!(first < second, "lines must read oldest to newest:\n{text}");
}

#[test]
fn the_command_line_shows_what_is_being_typed_with_its_colon() {
    let mut app = registered_app();
    app.input = tui::app::state::InputMode::Command("devices".to_string());

    let text = screen(&app, 80, 20);

    assert!(text.contains(":devices"), "got:\n{text}");
}

#[test]
fn a_pending_confirmation_replaces_the_bottom_line() {
    let mut app = registered_app();
    app.confirm = Some(tui::app::state::Confirm {
        prompt: text_of("confirm.forget"),
        kind: tui::app::state::ConfirmKind::ForgetIdentity,
    });

    let text = screen(&app, 80, 20);

    assert!(
        text.contains("[y/n]"),
        "the prompt must be visible:\n{text}"
    );
}

#[test]
fn the_chat_column_explains_an_empty_list() {
    let app = registered_app();

    let text = screen(&app, 80, 20);

    assert!(
        text.contains("invitation"),
        "an empty chat list must say why it is empty:\n{text}"
    );
}

#[test]
fn the_chat_column_is_hidden_in_a_conversation() {
    let mut app = registered_app();
    app.chats = vec![serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(),
        "created_at": "2026-10-01T10:00:00Z",
        "history": { "2026-10-01T10:00:00Z": { "name": "anna", "chat_type": 0, "members": {} } },
        "messages": {}
    }))
    .unwrap()];

    let with_column = screen(&app, 80, 20);
    assert!(with_column.contains("anna"), "the list must show the chat");

    app.view = View::Conversation(app.chats[0].id);
    let focused = screen(&app, 80, 20);

    assert!(
        !focused.contains(text_of("ui.chats_title").trim()),
        "focusing a chat must collapse the column:\n{focused}"
    );
}

#[test]
fn the_devices_view_lists_devices_and_marks_this_one() {
    let mut app = registered_app();
    let own = app.identity.device_id.unwrap();
    app.devices = vec![device(own), device(Uuid::new_v4())];
    app.view = View::Devices;

    let text = screen(&app, 80, 20);

    assert!(text.contains(&app.devices[0].short_id()), "got:\n{text}");
    assert!(text.contains(&app.devices[1].short_id()), "got:\n{text}");
    assert!(
        text.contains(text_of("ui.devices_this_one").trim()),
        "the local device must be marked:\n{text}"
    );
    assert!(
        text.contains(&text_of("ui.devices_key_link")),
        "the link-token hint must be in the list, not just the status bar:\n{text}"
    );
    assert!(
        text.contains(&text_of("ui.devices_key_revoke")),
        "the revoke hint must be in the list:\n{text}"
    );
}

#[test]
fn the_link_token_view_shows_a_qr_the_token_and_a_countdown() {
    let mut app = registered_app();
    app.link_token = Some(LinkTokenView {
        user_id: Uuid::new_v4(),
        token: "kJ3n9xToken".to_string(),
        expires_at: app.now + Duration::seconds(272),
        user_seed: "c2VlZA".to_string(),
    });
    app.view = View::LinkToken;

    let text = screen(&app, 80, 30);

    assert!(
        text.contains("kJ3n9xToken"),
        "the token must be copyable:\n{text}"
    );
    assert!(text.contains("4:32"), "a countdown must be shown:\n{text}");
    assert!(
        text.chars()
            .any(|character| "█▀▄ ".contains(character) && character != ' '),
        "half-block QR glyphs must be present:\n{text}"
    );
}

#[test]
fn an_expired_link_token_says_so_instead_of_showing_a_countdown() {
    let mut app = registered_app();
    app.link_token = Some(LinkTokenView {
        user_id: Uuid::new_v4(),
        token: "kJ3n9xToken".to_string(),
        expires_at: app.now - Duration::seconds(1),
        user_seed: "c2VlZA".to_string(),
    });
    app.view = View::LinkToken;

    let text = screen(&app, 80, 30);

    assert!(
        text.contains(&text_of("ui.qr_expired")),
        "an expired token must say so:\n{text}"
    );
    assert!(
        !text.contains(&prefix("ui.qr_expires_in")),
        "an expired token must not also show a countdown:\n{text}"
    );
}

#[test]
fn the_conversation_view_shows_messages_and_the_message_prompt() {
    let mut app = registered_app();
    let chat_id = Uuid::new_v4();
    app.view = View::Conversation(chat_id);
    app.input = tui::app::state::InputMode::Message("a draft".to_string());
    app.messages = vec![
        serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "author_id": app.identity.user_id.unwrap(),
            "chat_id": chat_id,
            "history": { "2026-10-01T10:00:00Z": { "id": Uuid::new_v4(), "text": "my message", "attachments": [], "reactions": [] } },
            "seen_by": {},
            "created_at": "2026-10-01T10:00:00Z",
            "updated_at": "2026-10-01T10:00:00Z",
            "available_to": null, "hidden": false, "is_reply": false, "reply_to_id": null
        }))
        .unwrap(),
    ];

    let text = screen(&app, 80, 20);

    assert!(text.contains("my message"), "got:\n{text}");
    assert!(
        text.contains("a draft"),
        "the draft must be visible:\n{text}"
    );
    assert!(
        text.contains(&format!("{:<10}", text_of("ui.conversation_me"))),
        "the local author must be labelled on the message row, padded to the column width so \
         the bare word cannot match somewhere else:\n{text}"
    );
}

#[test]
fn rendering_survives_a_tiny_terminal() {
    let mut app = registered_app();
    app.log(StreamKind::Info, "a line");
    app.view = View::Devices;
    app.devices = vec![device(Uuid::new_v4())];

    for (width, height) in [(1, 1), (4, 2), (20, 5)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| tui::ui::draw(frame, &app)).unwrap();
    }
}

#[test]
fn the_raw_stream_says_so_in_its_title() {
    let mut app = registered_app();
    app.log(StreamKind::Info, "a line");
    let marker = text_of("stream.title_raw").trim().to_string();

    let readable = screen(&app, 80, 20);
    assert!(!readable.contains(&marker), "got:\n{readable}");

    app.raw_stream = true;
    let raw = screen(&app, 80, 20);
    assert!(
        raw.contains(&marker),
        "raw mode must be visible in the title:\n{raw}"
    );
}

#[test]
fn a_confirmation_hides_the_draft_it_interrupts() {
    let mut app = registered_app();
    app.view = View::Conversation(Uuid::new_v4());
    app.input = tui::app::state::InputMode::Message("an unsent draft".to_string());

    let before = screen(&app, 80, 20);
    assert!(before.contains("an unsent draft"));

    app.confirm = Some(tui::app::state::Confirm {
        prompt: text_of("confirm.forget"),
        kind: tui::app::state::ConfirmKind::ForgetIdentity,
    });

    let after = screen(&app, 80, 20);
    assert!(after.contains("[y/n]"));
    assert!(
        !after.contains("an unsent draft"),
        "the prompt must own the line so the next key is unambiguous:\n{after}"
    );
}

#[test]
fn a_long_device_list_scrolls_to_keep_the_selection_visible() {
    let mut app = registered_app();
    app.devices = (0..30).map(|_| device(Uuid::new_v4())).collect();
    app.device_cursor = 25;
    app.view = View::Devices;

    let text = screen(&app, 80, 12);

    assert!(
        text.contains(&app.devices[25].short_id()),
        "the selected device must be on screen:\n{text}"
    );
    assert!(
        !text.contains(&app.devices[0].short_id()),
        "the list must have scrolled past the first device:\n{text}"
    );
}

#[test]
fn a_long_chat_list_scrolls_to_keep_the_selection_visible() {
    let mut app = registered_app();
    app.chats = (0..30)
        .map(|index| {
            serde_json::from_value(serde_json::json!({
                "id": Uuid::new_v4(),
                "created_at": "2026-10-01T10:00:00Z",
                "history": { "2026-10-01T10:00:00Z": { "name": format!("chat-{index:02}"), "chat_type": 0, "members": {} } },
                "messages": {}
            }))
            .unwrap()
        })
        .collect();
    app.chat_cursor = 25;

    let text = screen(&app, 80, 12);

    assert!(text.contains("chat-25"), "got:\n{text}");
    assert!(
        !text.contains("chat-00"),
        "the list must have scrolled:\n{text}"
    );
}

#[test]
fn the_identity_survives_a_long_disconnect_reason() {
    let mut app = registered_app();
    app.connection = ConnectionStatus::Disconnected {
        reason: "x".repeat(200),
    };

    let text = screen(&app, 80, 20);

    assert!(
        text.contains(&app.identity.short_label().unwrap()),
        "a long reason must not push the identity off the bar:\n{text}"
    );
}

#[test]
fn the_status_bar_hint_follows_the_identity_state() {
    let mut app = App::new();
    let unregistered = screen(&app, 100, 20);
    assert!(
        unregistered.contains(":register"),
        "an unregistered user must be told where to start:\n{unregistered}"
    );

    let registered = screen(&registered_app(), 100, 20);
    assert!(
        !registered.contains(text_of("status.keys_unregistered").trim()),
        "a registered user must not be told to register:\n{registered}"
    );
    assert!(
        registered.contains(":devices"),
        "a registered user needs the commands that apply to them:\n{registered}"
    );

    app.identity = IdentityInfo {
        state: "conflict".to_string(),
        user_id: Some(Uuid::new_v4()),
        device_id: Some(Uuid::new_v4()),
    };
    let conflicted = screen(&app, 100, 20);
    assert!(
        conflicted.contains(":forget"),
        "a conflicted identity must be pointed at its only way out:\n{conflicted}"
    );
}

#[test]
fn the_palette_is_painted_rather_than_left_to_the_terminal() {
    let app = registered_app();
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    terminal.draw(|frame| tui::ui::draw(frame, &app)).unwrap();

    let cells: Vec<_> = terminal.backend().buffer().content().to_vec();

    assert!(
        cells.iter().all(|cell| cell.bg != Color::Reset),
        "a cell left on the terminal default means the app background was not filled"
    );
    assert!(
        cells.iter().any(|cell| cell.bg == theme::BG_SURFACE),
        "the stream pane must sit on the surface colour"
    );
    assert!(
        cells.iter().any(|cell| cell.bg == theme::BG_SIDEBAR),
        "the chat column and status bar must sit on the sidebar colour"
    );
    assert!(
        cells.iter().any(|cell| cell.fg == theme::PRIMARY),
        "pane titles must use the primary colour"
    );
    assert!(
        cells.iter().any(|cell| cell.fg == theme::BORDER),
        "borders must use the palette, not the terminal default"
    );
}

#[test]
fn the_command_box_is_absent_until_a_command_is_opened() {
    let mut app = registered_app();
    app.log(StreamKind::Info, "a line");
    let title = text_of("ui.command_title");

    let idle = screen(&app, 60, 12);
    assert!(
        !idle.contains(&title),
        "the command box must not be on screen at rest:\n{idle}"
    );

    app.input = tui::app::state::InputMode::Command("dev".to_string());
    let open = screen(&app, 60, 12);
    assert!(
        open.contains(&title),
        "opening it must give it its own titled box:\n{open}"
    );
    assert!(open.contains(":dev"), "got:\n{open}");
}

#[test]
fn a_conversation_shows_the_message_box_instead() {
    let mut app = registered_app();
    app.view = View::Conversation(Uuid::new_v4());
    app.input = tui::app::state::InputMode::Message(String::new());

    let text = screen(&app, 60, 12);

    assert!(
        text.contains(&text_of("ui.message_title")),
        "a chat is for writing, so its box stays up:\n{text}"
    );
}

#[test]
fn the_focused_pane_is_the_one_with_the_highlighted_border() {
    fn border_colours(app: &App) -> Vec<Color> {
        let mut terminal = Terminal::new(TestBackend::new(60, 10)).unwrap();
        terminal.draw(|frame| tui::ui::draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..60).map(|x| buffer[(x, 0)].fg).collect()
    }

    let mut app = registered_app();
    app.focus = tui::app::state::Focus::Main;
    let main_focused = border_colours(&app);

    app.focus = tui::app::state::Focus::Chats;
    let chats_focused = border_colours(&app);

    assert_eq!(
        main_focused[18],
        theme::BORDER,
        "the chat column is not focused"
    );
    assert_eq!(
        main_focused[55],
        theme::HIGHLIGHT,
        "the main pane is focused"
    );

    assert_eq!(
        chats_focused[18],
        theme::HIGHLIGHT,
        "focus moved to the chats"
    );
    assert_eq!(chats_focused[55], theme::BORDER, "the main pane lost it");
}

#[test]
fn a_marked_up_chord_is_styled_differently_from_the_text_around_it() {
    let app = registered_app();
    let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();
    terminal.draw(|frame| tui::ui::draw(frame, &app)).unwrap();

    let buffer = terminal.backend().buffer();
    let last = 9;
    let row: String = (0..80).map(|x| buffer[(x, last)].symbol()).collect();

    let at = row
        .char_indices()
        .position(|(byte, _)| row[byte..].starts_with(":chats"))
        .unwrap_or_else(|| panic!("got {row:?}")) as u16;

    assert_eq!(
        buffer[(at, last)].fg,
        theme::TOKEN_KEY,
        "a command name must carry the token colour"
    );
    assert!(
        buffer[(at, last)]
            .modifier
            .contains(ratatui::style::Modifier::BOLD),
        "and its own weight"
    );
    let gap = at + ":chats".len() as u16;
    assert_eq!(
        buffer[(gap, last)].fg,
        theme::TEXT_MUTED,
        "the text around it stays plain, so the two really are different styles"
    );
}

fn locked_app() -> App {
    let mut app = App::new();
    app.connection = ConnectionStatus::Connected;
    app.identity = IdentityInfo {
        state: "locked".to_string(),
        user_id: None,
        device_id: None,
    };
    app.view = View::Unlock;
    app.locked = true;
    app.input = tui::app::state::InputMode::Pin(String::new());
    app.focus = tui::app::state::Focus::Command;
    app
}

#[test]
fn the_locked_view_explains_itself_and_masks_the_pin() {
    let mut app = locked_app();
    app.input = tui::app::state::InputMode::Pin("1234".to_string());

    let text = screen(&app, 80, 20);

    assert!(
        text.contains(text_of("ui.locked_title").trim()),
        "the pane must say what is going on:\n{text}"
    );
    assert!(text.contains("PIN"), "and name what it wants:\n{text}");
    assert!(
        !text.contains("1234"),
        "the PIN must never be drawn:\n{text}"
    );
    assert!(
        text.contains("••••"),
        "but its length must be visible, so typing is not blind:\n{text}"
    );
}

#[test]
fn the_chat_column_is_hidden_while_the_client_is_locked() {
    let app = locked_app();

    let text = screen(&app, 80, 20);

    assert!(
        !text.contains(text_of("ui.chats_title").trim()),
        "an empty chat list beside a shut vault is noise:\n{text}"
    );
    assert!(
        !text.contains("No identity"),
        "and a locked vault is not a missing identity:\n{text}"
    );
}

#[test]
fn the_invitations_view_shows_a_pending_invitation() {
    let mut app = registered_app();
    app.invitations = vec![
        serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "chat_id": Uuid::new_v4(),
            "inviter_user_id": Uuid::new_v4(),
            "invitee_user_id": Uuid::new_v4(),
            "direction": 1,
            "status": 0,
            "created_at": "2026-10-01T10:00:00Z"
        }))
        .unwrap(),
    ];
    app.view = View::Invitations;

    let text = screen(&app, 80, 20);

    assert!(
        text.contains(&text_of("ui.invitation_status_pending")),
        "got:\n{text}"
    );
}

#[test]
fn the_invitations_view_explains_an_empty_list() {
    let mut app = registered_app();
    app.view = View::Invitations;

    let text = screen(&app, 80, 20);

    assert!(
        text.contains(text_of("ui.invitations_empty").split('\n').next().unwrap()),
        "got:\n{text}"
    );
}

#[test]
fn the_contact_code_view_shows_the_code_and_a_qr() {
    let mut app = registered_app();
    app.contact_code = Some(tui::ipc::protocol::ContactCodeView {
        user_id: Uuid::new_v4(),
        fingerprint: "a".repeat(43),
        code: "ABCDEF-contact-code".to_string(),
    });
    app.view = View::ContactCode;

    let text = screen(&app, 80, 30);

    assert!(text.contains("ABCDEF-contact-code"), "got:\n{text}");
    assert!(
        text.chars()
            .any(|character| "█▀▄ ".contains(character) && character != ' '),
        "half-block QR glyphs must be present:\n{text}"
    );
}

#[test]
fn the_status_bar_asks_for_the_pin_while_locked() {
    let app = locked_app();

    let text = screen(&app, 80, 20);
    let hint = text_of("status.keys_unlock");

    assert!(text.contains(hint.trim()), "expected {hint:?} in:\n{text}");
}
