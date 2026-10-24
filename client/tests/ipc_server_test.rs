mod fixtures;

use std::time::Duration;

use client::adapters::r#in::ipc::server;
use client::domain::state::connectivity::ConnectivityStatus;
use client::domain::state::facade_events::{FacadeEvent, FacadeEventBus, facade_event_bus};
use client::domain::state::identity::IdentityStatus;
use fixtures::setup_db;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync;
use uuid::Uuid;

async fn start_test_server() -> (
    std::path::PathBuf,
    FacadeEventBus,
    sync::watch::Sender<bool>,
) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    let db = setup_db().await;
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let facade = fixtures::build_facade(
        &db,
        IdentityStatus::Registered(fixtures::identity_with(user_id, device_id)),
        None,
    )
    .await
    .facade;

    let events = facade_event_bus();
    let socket_path = std::env::temp_dir().join(format!("e2e-test-{}.sock", Uuid::new_v4()));

    let server_events = events.clone();
    let server_path = socket_path.clone();
    tokio::spawn(async move {
        if let Err(e) = server::run(&server_path, facade, server_events, rx).await {
            panic!("Error {e}");
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    (socket_path, events, tx)
}

async fn connect_and_init(
    socket_path: &std::path::Path,
) -> (Lines<BufReader<OwnedReadHalf>>, OwnedWriteHalf) {
    let stream = UnixStream::connect(socket_path).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();

    let mut lines = BufReader::new(read_half).lines();
    let init =
        serde_json::json!({"id": 0, "method": "session.init", "params": { "protocol_version": 2 }});

    write_half
        .write_all(format!("{init}\n").as_bytes())
        .await
        .unwrap();

    let line = lines
        .next_line()
        .await
        .unwrap()
        .expect("server has been closed during handshake");

    let response: Value = serde_json::from_str(&line).unwrap();
    assert!(
        response.get("result").is_some(),
        "handshake has been rejected: {response}"
    );

    (lines, write_half)
}

#[tokio::test]
async fn a_request_over_the_socket_gets_a_matching_response() {
    let (socket_path, _events, _tx) = start_test_server().await;
    let (mut lines, mut write_half) = connect_and_init(&socket_path).await;

    write_half
        .write_all(b"{\"id\":1,\"method\":\"identity.get\",\"params\":{}}\n")
        .await
        .unwrap();

    let line = lines.next_line().await.unwrap().unwrap();
    let response: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(response["id"], json!(1));
    assert!(response.get("result").is_some());
}

#[tokio::test]
async fn a_published_event_arrives_as_a_notification() {
    let (socket_path, events, _tx) = start_test_server().await;
    let (mut lines, _write_half) = connect_and_init(&socket_path).await;

    events
        .send(FacadeEvent::DeliveryReceiptMarked {
            message_id: Uuid::new_v4(),
            recipient_id: Uuid::new_v4(),
        })
        .unwrap();

    let line = lines.next_line().await.unwrap().unwrap();
    let notification: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(notification["method"], "message.delivered");
}

#[tokio::test]
async fn disconnecting_drops_the_connections_event_subscription() {
    let (socket_path, events, _tx) = start_test_server().await;

    assert_eq!(
        events.receiver_count(),
        0,
        "no client connected yet, so no connection task should be subscribed"
    );

    let (lines, write_half) = connect_and_init(&socket_path).await;

    assert_eq!(
        events.receiver_count(),
        1,
        "the connection task should have subscribed to the event bus"
    );

    drop(lines);
    drop(write_half);

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        events.receiver_count(),
        0,
        "the connection task's broadcast subscription should be dropped on disconnect, \
         not leaked"
    );
}

#[tokio::test]
async fn publishing_after_a_client_disconnects_does_not_hang_or_panic() {
    let (socket_path, events, _tx) = start_test_server().await;
    let stream = UnixStream::connect(&socket_path).await.unwrap();

    drop(stream);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let publish = tokio::time::timeout(Duration::from_secs(2), async {
        let _ = events.send(FacadeEvent::ConnectivityChanged(ConnectivityStatus::Online));
    })
    .await;

    assert!(
        publish.is_ok(),
        "publishing a facade event after client disconnect timed out"
    );

    tokio::time::sleep(Duration::from_millis(50)).await;
}
