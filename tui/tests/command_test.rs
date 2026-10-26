use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

use tui::app::command::{Command, HELP, parse};
use tui::app::state::{App, ConfirmKind, Focus, HelpPlacement, InputMode, StreamKind, View};
use tui::app::update::{Event, update};
use tui::i18n::t;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn type_line(app: &mut App, text: &str) -> Vec<tui::ipc::protocol::Request> {
    let mut requests = Vec::new();
    if app.input == InputMode::Idle && !text.trim_start().starts_with(':') {
        requests.extend(update(app, key(KeyCode::Char(':'))));
    }
    for character in text.chars() {
        requests.extend(update(app, key(KeyCode::Char(character))));
    }
    requests.extend(update(app, key(KeyCode::Enter)));
    requests
}

#[test]
fn it_parses_the_argument_free_commands() {
    assert_eq!(parse("register").unwrap(), Command::Register);
    assert_eq!(parse("devices").unwrap(), Command::Devices);
    assert_eq!(parse("link-token").unwrap(), Command::LinkToken);
    assert_eq!(parse("chats").unwrap(), Command::Chats);
    assert_eq!(parse("clear").unwrap(), Command::Clear);
    assert_eq!(parse("cls").unwrap(), Command::Clear);
    assert_eq!(parse("forget").unwrap(), Command::Forget);
    assert_eq!(parse("debug").unwrap(), Command::Debug);
    assert_eq!(parse("quit").unwrap(), Command::Quit);
    assert_eq!(parse("exit").unwrap(), Command::Quit);
    assert_eq!(parse(":exit").unwrap(), Command::Quit);
    assert_eq!(parse("q").unwrap(), Command::Quit);
}

#[test]
fn it_tolerates_surrounding_whitespace_and_is_case_insensitive() {
    assert_eq!(parse("  DEVICES  ").unwrap(), Command::Devices);
}

#[test]
fn it_parses_link_with_a_user_id_and_a_token() {
    let user_id = Uuid::new_v4();
    let parsed = parse(&format!("link {user_id} kJ3n9x")).unwrap();

    assert_eq!(
        parsed,
        Command::Link {
            user_id,
            token: "kJ3n9x".to_string()
        }
    );
}

#[test]
fn link_rejects_a_bad_uuid_and_a_missing_token() {
    assert!(parse("link not-a-uuid kJ3n9x").is_err());
    assert!(parse(&format!("link {}", Uuid::new_v4())).is_err());
}

#[test]
fn it_parses_revoke_with_a_device_id() {
    let device_id = Uuid::new_v4();
    assert_eq!(
        parse(&format!("revoke {device_id}")).unwrap(),
        Command::Revoke { device_id }
    );
    assert!(parse("revoke").is_err());
}

#[test]
fn an_unknown_command_is_an_error_naming_itself() {
    let error = parse("frobnicate").unwrap_err();
    assert!(error.contains("frobnicate"), "got {error}");
}

#[test]
fn an_empty_command_is_an_error() {
    assert!(parse("").is_err());
}

#[test]
fn typing_a_command_sends_the_matching_request() {
    let mut app = App::new();

    let requests = type_line(&mut app, "devices");

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "devices.list");
    assert_eq!(
        app.input,
        InputMode::Idle,
        "the line must hide itself again after it runs"
    );
}

#[test]
fn backspace_edits_the_command_line() {
    let mut app = App::new();

    update(&mut app, key(KeyCode::Char(':')));
    update(&mut app, key(KeyCode::Char('d')));
    update(&mut app, key(KeyCode::Char('x')));
    update(&mut app, key(KeyCode::Backspace));
    update(&mut app, key(KeyCode::Char('e')));

    assert_eq!(app.input, InputMode::Command("de".to_string()));
}

#[test]
fn quit_sets_the_flag() {
    let mut app = App::new();

    type_line(&mut app, "quit");

    assert!(app.should_quit);
}

#[test]
fn debug_toggles_the_raw_stream() {
    let mut app = App::new();
    assert!(!app.raw_stream);

    type_line(&mut app, "debug");
    assert!(app.raw_stream);

    type_line(&mut app, "debug");
    assert!(!app.raw_stream);
}

#[test]
fn revoke_asks_for_confirmation_before_sending_anything() {
    let mut app = App::new();
    let device_id = Uuid::new_v4();

    let requests = type_line(&mut app, &format!("revoke {device_id}"));

    assert!(requests.is_empty(), "nothing may be sent before confirming");
    assert_eq!(
        app.confirm.as_ref().map(|confirm| confirm.kind),
        Some(ConfirmKind::RevokeDevice(device_id))
    );

    let requests = update(&mut app, key(KeyCode::Char('y')));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "devices.revoke");
    assert!(app.confirm.is_none());
}

#[test]
fn declining_a_confirmation_sends_nothing() {
    let mut app = App::new();
    type_line(&mut app, &format!("revoke {}", Uuid::new_v4()));

    let requests = update(&mut app, key(KeyCode::Char('n')));

    assert!(requests.is_empty());
    assert!(app.confirm.is_none());
}

#[test]
fn in_a_conversation_the_bottom_line_sends_messages() {
    let mut app = App::new();
    let chat_id = Uuid::new_v4();
    app.view = View::Conversation(chat_id);
    app.input = InputMode::Message(String::new());

    let requests = type_line(&mut app, "hej");

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "messages.send");
    assert_eq!(requests[0].params["chat_id"], serde_json::json!(chat_id));
    assert_eq!(requests[0].params["text"], serde_json::json!("hej"));
}

#[test]
fn a_leading_colon_switches_from_message_to_command() {
    let mut app = App::new();
    app.view = View::Conversation(Uuid::new_v4());
    app.input = InputMode::Message(String::new());

    update(&mut app, key(KeyCode::Char(':')));

    assert_eq!(app.input, InputMode::Command(String::new()));
}

#[test]
fn a_doubled_colon_types_a_literal_colon_into_a_message() {
    let mut app = App::new();
    app.view = View::Conversation(Uuid::new_v4());
    app.input = InputMode::Message(String::new());

    update(&mut app, key(KeyCode::Char(':')));
    update(&mut app, key(KeyCode::Char(':')));

    assert_eq!(
        app.input,
        InputMode::Message(":".to_string()),
        "`::` must escape to a literal colon so a message can start with one"
    );
}

#[test]
fn a_colon_inside_a_message_is_just_text() {
    let mut app = App::new();
    app.view = View::Conversation(Uuid::new_v4());
    app.input = InputMode::Message(String::new());

    update(&mut app, key(KeyCode::Char('a')));
    update(&mut app, key(KeyCode::Char(':')));

    assert_eq!(app.input, InputMode::Message("a:".to_string()));
}

#[test]
fn escape_leaves_a_conversation_for_the_chat_list() {
    let mut app = App::new();
    app.view = View::Conversation(Uuid::new_v4());
    app.input = InputMode::Message(String::new());

    update(&mut app, key(KeyCode::Esc));

    assert!(matches!(app.view, View::Stream));
    assert_eq!(app.input, InputMode::Idle);
}

#[test]
fn in_the_devices_view_l_and_r_are_shortcuts() {
    let mut app = App::new();
    app.view = View::Devices;
    app.devices = vec![
        serde_json::from_value(serde_json::json!({
            "device_id": Uuid::new_v4(),
            "keys": { "signing_key": "a".repeat(43), "agreement_key": "b".repeat(43) },
            "registered_at": "2026-10-24T12:55:40.855Z"
        }))
        .unwrap(),
    ];

    let requests = update(&mut app, key(KeyCode::Char('l')));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "devices.create_link_token");

    let requests = update(&mut app, key(KeyCode::Char('r')));
    assert!(requests.is_empty(), "revoking must confirm first");
    assert!(app.confirm.is_some());
}

#[test]
fn arrows_move_the_device_cursor_without_going_out_of_bounds() {
    let mut app = App::new();
    app.view = View::Devices;
    app.devices = (0..2)
        .map(|_| {
            serde_json::from_value(serde_json::json!({
                "device_id": Uuid::new_v4(),
                "keys": { "signing_key": "a".repeat(43), "agreement_key": "b".repeat(43) },
                "registered_at": "2026-10-24T12:55:40.855Z"
            }))
            .unwrap()
        })
        .collect();

    update(&mut app, key(KeyCode::Up));
    assert_eq!(app.device_cursor, 0, "must not wrap below zero");

    update(&mut app, key(KeyCode::Down));
    assert_eq!(app.device_cursor, 1);

    update(&mut app, key(KeyCode::Down));
    assert_eq!(app.device_cursor, 1, "must not run past the last device");
}

#[test]
fn enter_on_a_chat_opens_it() {
    let mut app = App::new();
    app.chats = vec![serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(),
        "created_at": "2026-10-01T10:00:00Z",
        "history": { "2026-10-01T10:00:00Z": { "name": "anna", "chat_type": 0, "members": {} } },
        "messages": {}
    }))
    .unwrap()];
    let chat_id = app.chats[0].id;

    let requests = update(&mut app, key(KeyCode::Tab));
    assert!(requests.is_empty(), "tab only moves focus to the list");

    let requests = update(&mut app, key(KeyCode::Enter));

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "messages.list");
    assert_eq!(
        requests[0].params["filter"]["chat_id"],
        serde_json::json!(chat_id)
    );
}

#[test]
fn opening_a_chat_accepts_a_message_before_the_list_response_lands() {
    let mut app = App::new();
    app.chats = vec![
        serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "created_at": "2026-10-01T10:00:00Z",
            "history": { "2026-10-01T10:00:00Z": { "name": "anna", "chat_type": 0, "members": {} } },
            "messages": {}
        }))
        .unwrap(),
    ];
    let chat_id = app.chats[0].id;

    let requests = update(&mut app, key(KeyCode::Enter));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "messages.list");
    assert_eq!(app.focused_chat(), Some(chat_id));

    let requests = type_line(&mut app, "hej");
    assert_eq!(requests.len(), 1, "the message must be sent, not refused");
    assert_eq!(requests[0].method, "messages.send");
    assert_eq!(requests[0].params["chat_id"], serde_json::json!(chat_id));
}

fn device_fixture() -> tui::ipc::protocol::DeviceView {
    serde_json::from_value(serde_json::json!({
        "device_id": Uuid::new_v4(),
        "keys": { "signing_key": "a".repeat(43), "agreement_key": "b".repeat(43) },
        "registered_at": "2026-10-24T12:55:40.855Z"
    }))
    .unwrap()
}

#[test]
fn the_commands_the_help_prints_are_typeable_verbatim() {
    let mut app = App::new();

    let requests = type_line(&mut app, ":register");
    assert_eq!(requests.len(), 1, "got {requests:?}");
    assert_eq!(requests[0].method, "identity.register");

    let user_id = Uuid::new_v4();
    let requests = type_line(&mut app, &format!(":link {user_id} kJ3n9x"));
    assert_eq!(requests.len(), 1, "got {requests:?}");
    assert_eq!(requests[0].method, "identity.link");
}

#[test]
fn a_command_can_be_typed_in_the_devices_view_without_the_shortcuts_eating_it() {
    let mut app = App::new();
    app.view = View::Devices;
    app.devices = vec![device_fixture()];

    let requests = type_line(&mut app, ":link-token");
    assert_eq!(
        requests.len(),
        1,
        "the typed command must run, not the shortcut: {requests:?}"
    );
    assert_eq!(requests[0].method, "devices.create_link_token");
    assert!(app.confirm.is_none(), "no shortcut may have fired");

    let requests = type_line(&mut app, ":register");
    assert_eq!(requests.len(), 1, "got {requests:?}");
    assert_eq!(requests[0].method, "identity.register");
    assert!(
        app.confirm.is_none(),
        "typing `register` must not arm the revoke confirmation"
    );
}

#[test]
fn a_key_release_is_ignored_so_characters_are_not_typed_twice() {
    use ratatui::crossterm::event::KeyEventKind;

    let mut app = App::new();
    let press = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
    let mut release = press;
    release.kind = KeyEventKind::Release;

    update(&mut app, key(KeyCode::Char(':')));
    update(&mut app, Event::Key(press));
    update(&mut app, Event::Key(release));

    assert_eq!(app.input, InputMode::Command("d".to_string()));
}

#[test]
fn a_key_release_cannot_answer_a_confirmation() {
    use ratatui::crossterm::event::KeyEventKind;

    let mut app = App::new();
    type_line(&mut app, &format!("revoke {}", Uuid::new_v4()));
    assert!(app.confirm.is_some());

    let mut release = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    let requests = update(&mut app, Event::Key(release));

    assert!(requests.is_empty(), "a release must send nothing");
    assert!(
        app.confirm.is_some(),
        "the prompt must survive a release and still await a real answer"
    );
}

#[test]
fn a_modified_y_does_not_confirm_a_destructive_action() {
    let mut app = App::new();
    type_line(&mut app, &format!("revoke {}", Uuid::new_v4()));

    let requests = update(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)),
    );

    assert!(requests.is_empty(), "Ctrl+Y is not an answer: {requests:?}");
}

#[test]
fn navigating_devices_does_not_move_the_chat_selection() {
    let mut app = App::new();
    app.view = View::Devices;
    app.devices = vec![device_fixture(), device_fixture()];
    app.chats = (0..2)
        .map(|index| {
            serde_json::from_value(serde_json::json!({
                "id": Uuid::new_v4(),
                "created_at": "2026-10-01T10:00:00Z",
                "history": { "2026-10-01T10:00:00Z": { "name": format!("czat {index}"), "chat_type": 0, "members": {} } },
                "messages": {}
            }))
            .unwrap()
        })
        .collect();
    let first_chat = app.chats[0].id;

    update(&mut app, key(KeyCode::Down));

    assert_eq!(app.device_cursor, 1, "the visible list must move");
    assert_eq!(
        app.chat_cursor, 0,
        "the chat selection must not drift while navigating devices"
    );

    app.view = View::Stream;
    let requests = update(&mut app, key(KeyCode::Enter));
    assert_eq!(
        requests[0].params["filter"]["chat_id"],
        serde_json::json!(first_chat)
    );
}

#[test]
fn forgetting_an_identity_confirms_first_then_sends() {
    let mut app = App::new();

    let requests = type_line(&mut app, "forget");
    assert!(requests.is_empty(), "nothing may be sent before confirming");
    assert_eq!(
        app.confirm.as_ref().map(|confirm| confirm.kind),
        Some(ConfirmKind::ForgetIdentity)
    );

    let requests = update(&mut app, key(KeyCode::Char('y')));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "identity.forget");
}

#[test]
fn help_opens_the_window_in_the_centre_without_touching_the_view() {
    let mut app = App::new();
    app.view = View::Devices;

    let requests = type_line(&mut app, ":help");

    assert!(requests.is_empty(), "help talks to nobody: {requests:?}");
    assert_eq!(app.help, Some(HelpPlacement::Centre));
    assert_eq!(
        app.view,
        View::Devices,
        "the window floats over the view instead of replacing it"
    );
}

#[test]
fn alt_h_docks_the_help_on_the_left_and_toggles_it_off() {
    let mut app = App::new();
    assert_eq!(app.help, None);

    update(&mut app, alt(KeyCode::Char('h')));
    assert_eq!(app.help, Some(HelpPlacement::Left));

    update(&mut app, alt(KeyCode::Char('h')));
    assert_eq!(app.help, None, "the same chord closes it again");

    update(
        &mut app,
        Event::Key(KeyEvent::new(
            KeyCode::Char('H'),
            KeyModifiers::ALT | KeyModifiers::SHIFT,
        )),
    );
    assert_eq!(app.help, Some(HelpPlacement::Left));
}

#[test]
fn help_moves_from_the_dock_to_the_centre_when_the_command_runs() {
    let mut app = App::new();

    update(&mut app, alt(KeyCode::Char('h')));
    assert_eq!(app.help, Some(HelpPlacement::Left));

    type_line(&mut app, ":help");

    assert_eq!(
        app.help,
        Some(HelpPlacement::Centre),
        "the command moves the open window rather than opening a second one"
    );
}

#[test]
fn escape_closes_the_help_before_it_touches_the_view() {
    let mut app = App::new();
    app.view = View::Devices;
    app.help = Some(HelpPlacement::Centre);

    update(&mut app, key(KeyCode::Esc));
    assert_eq!(app.help, None);
    assert_eq!(app.view, View::Devices, "the view is left alone");

    update(&mut app, key(KeyCode::Esc));
    assert_eq!(app.view, View::Stream, "a second esc leaves the view");
}

#[test]
fn every_listed_command_is_one_the_parser_knows() {
    let usages = [t("command.usage_link"), t("command.usage_revoke")];

    for (spelling, _) in HELP {
        let verb = spelling.split_whitespace().next().unwrap();
        match parse(verb) {
            Ok(_) => {}
            Err(err) => assert!(
                usages.contains(&err),
                "{spelling} is listed but the parser rejects it: {err}"
            ),
        }
    }
}

#[test]
fn help_has_aliases_but_lists_only_the_canonical_spelling() {
    assert_eq!(parse("h").unwrap(), Command::Help);
    assert_eq!(parse("?").unwrap(), Command::Help);
    assert_eq!(parse(":help").unwrap(), Command::Help);
}

#[test]
fn the_echo_shows_one_colon_however_the_line_was_typed() {
    for typed in [":help", "help", "  :help  "] {
        let mut app = App::new();
        type_line(&mut app, typed);

        let echoed = app
            .stream
            .iter()
            .find(|line| line.text.starts_with(':'))
            .map(|line| line.text.clone())
            .unwrap_or_else(|| panic!("nothing was echoed for {typed:?}"));

        assert_eq!(echoed, ":help", "typed as {typed:?}");
    }
}

#[test]
fn clear_empties_the_stream_and_shows_it() {
    let mut app = App::new();
    app.view = View::Devices;
    for index in 0..5 {
        app.log(StreamKind::Info, format!("line {index}"));
    }

    let requests = type_line(&mut app, ":clear");

    assert!(
        requests.is_empty(),
        "clearing talks to nobody: {requests:?}"
    );
    assert_eq!(app.view, View::Stream);
    assert!(
        app.stream.is_empty(),
        "the echo of the command itself must go too, got {:?}",
        app.stream
    );
}

fn alt(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::ALT))
}

#[test]
fn the_command_line_starts_hidden_and_a_colon_opens_it() {
    let mut app = App::new();
    assert_eq!(app.input, InputMode::Idle);

    update(&mut app, key(KeyCode::Char('d')));
    assert_eq!(app.input, InputMode::Idle);

    update(&mut app, key(KeyCode::Char(':')));
    assert_eq!(
        app.input,
        InputMode::Command(String::new()),
        "the colon itself is the opener, so it is not part of the command"
    );

    update(&mut app, key(KeyCode::Char('h')));
    assert_eq!(app.input, InputMode::Command("h".to_string()));
}

#[test]
fn escape_and_backspace_both_close_the_command_line() {
    let mut app = App::new();

    update(&mut app, key(KeyCode::Char(':')));
    update(&mut app, key(KeyCode::Esc));
    assert_eq!(app.input, InputMode::Idle, "esc closes it");

    update(&mut app, key(KeyCode::Char(':')));
    update(&mut app, key(KeyCode::Char('h')));
    update(&mut app, key(KeyCode::Backspace));
    assert_eq!(
        app.input,
        InputMode::Idle,
        "backspacing past the start closes it too"
    );
}

#[test]
fn a_conversation_keeps_its_draft_line_rather_than_hiding_it() {
    let mut app = App::new();
    app.view = View::Conversation(Uuid::new_v4());
    app.input = InputMode::Message("half a word".to_string());

    update(&mut app, key(KeyCode::Enter));

    assert_eq!(
        app.input,
        InputMode::Message(String::new()),
        "a chat is for writing, so the draft line stays"
    );
}

#[test]
fn alt_arrows_walk_the_focus_cycle_in_screen_order() {
    let mut app = App::new();
    assert_eq!(app.focus, Focus::Main);

    update(&mut app, alt(KeyCode::Right));
    assert_eq!(app.focus, Focus::Command);
    update(&mut app, alt(KeyCode::Right));
    assert_eq!(app.focus, Focus::Chats, "the cycle wraps");
    update(&mut app, alt(KeyCode::Right));
    assert_eq!(app.focus, Focus::Main);

    update(&mut app, alt(KeyCode::Left));
    assert_eq!(app.focus, Focus::Chats, "left walks it backwards");
    update(&mut app, alt(KeyCode::Left));
    assert_eq!(app.focus, Focus::Command);

    update(&mut app, alt(KeyCode::Down));
    assert_eq!(app.focus, Focus::Chats, "both axes drive the same cycle");
    update(&mut app, alt(KeyCode::Up));
    assert_eq!(app.focus, Focus::Command);
}

#[test]
fn focusing_the_command_line_opens_it_and_leaving_an_empty_one_puts_it_away() {
    let mut app = App::new();
    assert_eq!(app.input, InputMode::Idle);

    update(&mut app, alt(KeyCode::Right));
    assert_eq!(app.focus, Focus::Command);
    assert_eq!(
        app.input,
        InputMode::Command(String::new()),
        "focusing the box has to open it, or focus would land on nothing"
    );

    update(&mut app, alt(KeyCode::Right));
    assert_eq!(app.input, InputMode::Idle, "an untouched box is put away");
}

#[test]
fn leaving_a_half_typed_command_keeps_it() {
    let mut app = App::new();

    update(&mut app, key(KeyCode::Char(':')));
    update(&mut app, key(KeyCode::Char('d')));
    update(&mut app, alt(KeyCode::Right));

    assert_eq!(
        app.input,
        InputMode::Command("d".to_string()),
        "typed text must survive a focus change"
    );
}

#[test]
fn typing_a_colon_also_moves_focus_to_the_command_line() {
    let mut app = App::new();
    app.focus = Focus::Chats;

    update(&mut app, key(KeyCode::Char(':')));

    assert_eq!(app.focus, Focus::Command);
}

#[test]
fn the_focused_command_line_keeps_the_arrows_to_itself() {
    let mut app = App::new();
    app.chats = vec![chat_fixture(), chat_fixture()];
    app.focus = Focus::Command;
    app.input = InputMode::Command(String::new());

    update(&mut app, key(KeyCode::Down));

    assert_eq!(
        app.chat_cursor, 0,
        "a list the user is not looking at must not move"
    );
}

#[test]
fn an_alt_arrow_moves_focus_instead_of_a_cursor() {
    let mut app = App::new();
    app.view = View::Devices;
    app.devices = vec![device_fixture(), device_fixture()];

    update(&mut app, alt(KeyCode::Down));

    assert_eq!(app.focus, Focus::Command);
    assert_eq!(
        app.device_cursor, 0,
        "the alt chord must not fall through to the list"
    );
}

#[test]
fn focus_decides_which_list_the_arrows_drive() {
    let mut app = App::new();
    app.view = View::Devices;
    app.devices = vec![device_fixture(), device_fixture()];
    app.chats = vec![chat_fixture(), chat_fixture()];

    update(&mut app, key(KeyCode::Down));
    assert_eq!(app.device_cursor, 1, "the main pane owns the arrows");
    assert_eq!(app.chat_cursor, 0);

    update(&mut app, alt(KeyCode::Left));
    update(&mut app, key(KeyCode::Down));
    assert_eq!(
        app.chat_cursor, 1,
        "with the chats focused the arrows move that list, even though devices are on screen"
    );
    assert_eq!(app.device_cursor, 1, "the other cursor must not move");
}

#[test]
fn escape_returns_focus_to_the_main_pane() {
    let mut app = App::new();
    update(&mut app, alt(KeyCode::Left));
    assert_eq!(app.focus, Focus::Chats);

    update(&mut app, key(KeyCode::Esc));

    assert_eq!(app.focus, Focus::Main);
}

fn chat_fixture() -> tui::ipc::protocol::ChatView {
    serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(),
        "created_at": "2026-10-01T10:00:00Z",
        "history": { "2026-10-01T10:00:00Z": { "name": "a chat", "chat_type": 0, "members": {} } },
        "messages": {}
    }))
    .unwrap()
}

fn locked_app() -> App {
    let mut app = App::new();
    app.locked = true;
    app.view = View::Unlock;
    app.input = InputMode::Pin(String::new());
    app.focus = Focus::Command;
    app
}

fn type_pin(app: &mut App, digits: &str) {
    for digit in digits.chars() {
        update(app, key(KeyCode::Char(digit)));
    }
}

#[test]
fn the_pin_line_takes_eight_digits_and_enter_sends_them() {
    let mut app = locked_app();

    type_pin(&mut app, "12ab345678999");

    assert_eq!(
        app.input,
        InputMode::Pin("12345678".to_string()),
        "only digits, and only as many as the PIN has"
    );

    let requests = update(&mut app, key(KeyCode::Enter));

    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].method, "vault.unlock");
    assert_eq!(requests[0].params["pin"], serde_json::json!("12345678"));
    assert_eq!(
        app.input,
        InputMode::Pin(String::new()),
        "the PIN must not linger in the state after it was sent"
    );
    assert!(
        !app.stream.iter().any(|line| line.text.contains("12345678")),
        "the PIN must never be echoed into the stream: {:?}",
        app.stream
    );
}

#[test]
fn a_short_pin_is_refused_without_a_round_trip() {
    let mut app = locked_app();
    type_pin(&mut app, "1234");

    let requests = update(&mut app, key(KeyCode::Enter));

    assert!(requests.is_empty(), "{requests:?}");
    assert!(
        app.stream
            .iter()
            .any(|line| line.text == t("unlock.bad_format")),
        "{:?}",
        app.stream
    );
}

#[test]
fn an_empty_pin_line_ignores_enter() {
    let mut app = locked_app();

    let requests = update(&mut app, key(KeyCode::Enter));

    assert!(requests.is_empty());
    assert!(app.stream.is_empty(), "{:?}", app.stream);
}

#[test]
fn backspace_edits_the_pin_line() {
    let mut app = locked_app();
    type_pin(&mut app, "1234");

    update(&mut app, key(KeyCode::Backspace));

    assert_eq!(app.input, InputMode::Pin("123".to_string()));

    for _ in 0..5 {
        update(&mut app, key(KeyCode::Backspace));
    }

    assert_eq!(
        app.input,
        InputMode::Pin(String::new()),
        "an empty PIN line stays, there is nowhere else to go"
    );
}

#[test]
fn a_colon_on_the_pin_line_opens_the_command_line_so_quit_still_works() {
    let mut app = locked_app();

    update(&mut app, key(KeyCode::Char(':')));
    assert_eq!(app.input, InputMode::Command(String::new()));

    let requests = type_line(&mut app, "quit");

    assert!(app.should_quit, "a locked client must still be quittable");
    assert!(requests.is_empty());
}

#[test]
fn escape_returns_from_the_command_line_to_the_pin_line() {
    let mut app = locked_app();
    update(&mut app, key(KeyCode::Char(':')));
    update(&mut app, key(KeyCode::Char('x')));

    update(&mut app, key(KeyCode::Esc));

    assert_eq!(app.input, InputMode::Pin(String::new()));
    assert_eq!(app.view, View::Unlock);
    assert_eq!(app.focus, Focus::Command);
}

#[test]
fn the_vault_commands_are_refused_while_the_client_is_locked() {
    let mut app = locked_app();
    update(&mut app, key(KeyCode::Char(':')));

    let requests = type_line(&mut app, "devices");

    assert!(
        requests.is_empty(),
        "the client would refuse it anyway: {requests:?}"
    );
    assert!(
        app.stream
            .iter()
            .any(|line| line.text == t("unlock.needed")),
        "{:?}",
        app.stream
    );
    assert_eq!(app.view, View::Unlock);
    assert_eq!(app.input, InputMode::Pin(String::new()));
}

#[test]
fn clearing_the_stream_while_locked_keeps_the_pin_in_front() {
    let mut app = locked_app();
    app.log(StreamKind::Info, "noise");
    update(&mut app, key(KeyCode::Char(':')));

    let requests = type_line(&mut app, "clear");

    assert!(requests.is_empty());
    assert!(app.stream.is_empty(), "{:?}", app.stream);
    assert_eq!(
        app.view,
        View::Unlock,
        "clearing the stream cannot hide the one thing that is needed"
    );
    assert_eq!(app.input, InputMode::Pin(String::new()));
}
