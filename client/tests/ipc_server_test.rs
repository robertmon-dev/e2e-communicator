mod fixtures;

use std::sync::Arc;
use std::time::Duration;

use client::adapters::r#in::ipc::facade::Facade;
use client::adapters::r#in::ipc::server;
use client::adapters::out::persistance::repositories::attachment::AttachmentRepository;
use client::adapters::out::persistance::repositories::chat::ChatRepository;
use client::adapters::out::persistance::repositories::key::KeyRepository;
use client::adapters::out::persistance::repositories::message::MessageRepository;
use client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use client::adapters::out::persistance::repositories::user::UserRepository;
use client::domain::services::exchanger::Exchanger;
use client::domain::services::messenger::Messenger;
use client::domain::state::connectivity::{ConnectivityState, ConnectivityStatus};
use client::domain::state::facade_events::{FacadeEvent, FacadeEventBus, facade_event_bus};
use fixtures::setup_db;
use orion::aead::SecretKey;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use uuid::Uuid;

async fn start_test_server() -> (std::path::PathBuf, FacadeEventBus) {
    let db = setup_db().await;
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let user_repository = Arc::new(UserRepository::new(db.clone()));
    let attachment_repository = Arc::new(AttachmentRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository, SecretKey::default()));
    let message_repository = Arc::new(MessageRepository::new(db.clone()));
    let chat_repository = Arc::new(ChatRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));
    let messenger = Arc::new(Messenger::new(
        message_repository.clone(),
        chat_repository.clone(),
        outbox_repository,
        exchanger,
    ));
    let facade = Arc::new(Facade::new(
        messenger,
        chat_repository,
        user_repository,
        message_repository,
        attachment_repository,
        Arc::new(ConnectivityState::default()),
        user_id,
        device_id,
        None,
    ));

    let events = facade_event_bus();
    let socket_path = std::env::temp_dir().join(format!("e2e-test-{}.sock", Uuid::new_v4()));

    let server_events = events.clone();
    let server_path = socket_path.clone();
    tokio::spawn(async move {
        server::run(&server_path, facade, server_events)
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    (socket_path, events)
}

#[tokio::test]
async fn a_request_over_the_socket_gets_a_matching_response() {
    let (socket_path, _events) = start_test_server().await;
    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

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
    let (socket_path, events) = start_test_server().await;
    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (read_half, _write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    tokio::time::sleep(Duration::from_millis(50)).await;

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
    let (socket_path, events) = start_test_server().await;

    assert_eq!(
        events.receiver_count(),
        0,
        "no client connected yet, so no connection task should be subscribed"
    );

    let stream = UnixStream::connect(&socket_path).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        events.receiver_count(),
        1,
        "the connection task should have subscribed to the event bus"
    );

    drop(stream);

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(
        events.receiver_count(),
        0,
        "the connection task's broadcast subscription should be dropped on disconnect, \
         not leaked"
    );
}

#[tokio::test]
async fn publishing_after_a_client_disconnects_does_not_hang_or_panic() {
    let (socket_path, events) = start_test_server().await;

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
