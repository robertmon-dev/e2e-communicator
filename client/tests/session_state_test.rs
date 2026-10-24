use chrono::{Duration, Utc};
use client::domain::state::session::{SessionState, SessionToken};

fn token(value: &str) -> SessionToken {
    SessionToken {
        token: value.to_string(),
        expires_at: Utc::now() + Duration::hours(1),
    }
}

#[test]
fn starts_unauthenticated() {
    let state = SessionState::default();
    assert!(state.get().is_none());
    assert!(!state.is_authenticated());
}

#[test]
fn set_and_clear_round_trip() {
    let state = SessionState::default();

    state.set(token("abc"));
    assert_eq!(state.get().unwrap().token, "abc");
    assert!(state.is_authenticated());

    state.clear();
    assert!(state.get().is_none());
}

#[tokio::test]
async fn subscribers_see_changes() {
    let state = SessionState::default();
    let mut rx = state.subscribe();

    state.set(token("abc"));
    rx.changed().await.unwrap();
    assert!(rx.borrow_and_update().is_some());

    state.clear();
    rx.changed().await.unwrap();
    assert!(rx.borrow_and_update().is_none());
}

#[test]
fn clearing_an_empty_session_does_not_notify() {
    let state = SessionState::default();
    let mut rx = state.subscribe();
    rx.borrow_and_update();

    state.clear();

    assert!(!rx.has_changed().unwrap());
}

#[test]
fn debug_output_redacts_the_token() {
    let rendered = format!("{:?}", token("super-secret"));
    assert!(!rendered.contains("super-secret"));
    assert!(rendered.contains("<redacted>"));
}
