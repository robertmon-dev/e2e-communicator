use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, watch};

use tui::ipc::connection::{self, Incoming};
use tui::ipc::protocol::Request;

fn socket_path() -> PathBuf {
    std::env::temp_dir().join(format!("tui-test-{}.sock", uuid::Uuid::new_v4()))
}

async fn fake_facade(listener: UnixListener, mut push: mpsc::Receiver<Value>) -> Vec<Value> {
    let (stream, _) = listener.accept().await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    let mut seen = Vec::new();

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { return seen };
                let request: Value = serde_json::from_str(&line).unwrap();
                let id = request["id"].clone();
                let result = if request["method"] == json!("session.init") {
                    json!({
                        "protocol_version": 2,
                        "identity_state": "unregistered",
                        "user_id": null,
                        "device_id": null,
                        "connectivity": "Online",
                        "authenticated": false
                    })
                } else {
                    json!({})
                };
                seen.push(request);
                let response = json!({ "id": id, "result": result }).to_string();
                if write_half.write_all(format!("{response}\n").as_bytes()).await.is_err() {
                    return seen;
                }
            }
            Some(note) = push.recv() => {
                let line = note.to_string();
                if write_half.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                    return seen;
                }
            }
        }
    }
}

#[tokio::test]
async fn it_handshakes_then_reports_responses_and_notifications() {
    let path = socket_path();
    let listener = UnixListener::bind(&path).unwrap();
    let (push_tx, push_rx) = mpsc::channel(4);
    let facade = tokio::spawn(fake_facade(listener, push_rx));

    let (event_tx, mut event_rx) = mpsc::channel(32);
    let (request_tx, request_rx) = mpsc::channel(32);
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        None,
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    assert!(matches!(
        event_rx.recv().await.unwrap(),
        Incoming::Connected
    ));

    match event_rx.recv().await.unwrap() {
        Incoming::Response { id, outcome } => {
            assert_eq!(id, connection::HANDSHAKE_ID);
            assert!(matches!(outcome, tui::ipc::protocol::Outcome::Ok { .. }));
        }
        other => panic!("expected the handshake response, got {other:?}"),
    }

    request_tx
        .send(Request {
            id: 42,
            method: "devices.list".to_string(),
            params: json!({}),
        })
        .await
        .unwrap();

    match event_rx.recv().await.unwrap() {
        Incoming::Response { id, .. } => assert_eq!(id, 42),
        other => panic!("expected a response for 42, got {other:?}"),
    }

    push_tx
        .send(json!({ "method": "session.changed", "params": { "authenticated": true } }))
        .await
        .unwrap();

    match event_rx.recv().await.unwrap() {
        Incoming::Notification { method, params } => {
            assert_eq!(method, "session.changed");
            assert_eq!(params["authenticated"], json!(true));
        }
        other => panic!("expected a notification, got {other:?}"),
    }

    task.abort();
    drop(push_tx);
    let seen = facade.await.unwrap();
    assert_eq!(seen[0]["method"], json!("session.init"));
    assert_eq!(seen[0]["params"]["protocol_version"], json!(2));
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn it_sends_the_session_token_when_one_is_configured() {
    let path = socket_path();
    let listener = UnixListener::bind(&path).unwrap();
    let (push_tx, push_rx) = mpsc::channel(4);
    let facade = tokio::spawn(fake_facade(listener, push_rx));

    let (event_tx, mut event_rx) = mpsc::channel(32);
    let (_request_tx, request_rx) = mpsc::channel(32);
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        Some("sekret".to_string()),
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    event_rx.recv().await.unwrap();
    event_rx.recv().await.unwrap();

    task.abort();
    drop(push_tx);
    let seen = facade.await.unwrap();
    assert_eq!(seen[0]["params"]["token"], json!("sekret"));
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn it_reports_a_missing_socket_then_connects_once_it_appears() {
    let path = socket_path();

    let (event_tx, mut event_rx) = mpsc::channel(32);
    let (_request_tx, request_rx) = mpsc::channel(32);
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        None,
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    match event_rx.recv().await.unwrap() {
        Incoming::Disconnected { .. } => {}
        other => panic!("expected Disconnected while the socket is absent, got {other:?}"),
    }

    let listener = UnixListener::bind(&path).unwrap();
    let (push_tx, push_rx) = mpsc::channel(4);
    let facade = tokio::spawn(fake_facade(listener, push_rx));

    let connected = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(event_rx.recv().await.unwrap(), Incoming::Connected) {
                return;
            }
        }
    })
    .await;
    assert!(connected.is_ok(), "the task must reconnect on its own");

    task.abort();
    drop(push_tx);
    let _ = facade.await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn it_reports_a_disconnect_when_the_facade_goes_away() {
    let path = socket_path();
    let listener = UnixListener::bind(&path).unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read_half, mut write_half) = stream.into_split();
        let mut lines = BufReader::new(read_half).lines();
        let line = lines.next_line().await.unwrap().unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        let response = json!({ "id": request["id"], "result": {
            "protocol_version": 2, "identity_state": "unregistered",
            "user_id": null, "device_id": null, "connectivity": "Online", "authenticated": false
        }})
        .to_string();
        write_half
            .write_all(format!("{response}\n").as_bytes())
            .await
            .unwrap();
    });

    let (event_tx, mut event_rx) = mpsc::channel(32);
    let (_request_tx, request_rx) = mpsc::channel(32);
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        None,
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    server.await.unwrap();

    let saw_disconnect = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(
                event_rx.recv().await.unwrap(),
                Incoming::Disconnected { .. }
            ) {
                return;
            }
        }
    })
    .await;
    assert!(saw_disconnect.is_ok(), "a closed socket must be reported");

    task.abort();
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn it_reports_a_malformed_line_instead_of_dying() {
    let path = socket_path();
    let listener = UnixListener::bind(&path).unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read_half, mut write_half) = stream.into_split();
        let mut lines = BufReader::new(read_half).lines();
        lines.next_line().await.unwrap();
        write_half
            .write_all(b"{ this is not json }\n")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    });

    let (event_tx, mut event_rx) = mpsc::channel(32);
    let (_request_tx, request_rx) = mpsc::channel(32);
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        None,
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    let saw_malformed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Incoming::Malformed { line } = event_rx.recv().await.unwrap() {
                assert!(line.contains("not json"));
                return;
            }
        }
    })
    .await;
    assert!(
        saw_malformed.is_ok(),
        "a bad line must be surfaced, not swallowed"
    );

    task.abort();
    server.abort();
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_socket_path_prefers_the_flag_then_the_env_then_the_default() {
    use tui::config::resolve_socket_path;

    assert_eq!(
        resolve_socket_path(
            Some(PathBuf::from("/tmp/a.sock")),
            Some("/tmp/b.sock".into())
        ),
        PathBuf::from("/tmp/a.sock")
    );
    assert_eq!(
        resolve_socket_path(None, Some("/tmp/b.sock".into())),
        PathBuf::from("/tmp/b.sock")
    );

    let fallback = resolve_socket_path(None, None);
    assert!(
        fallback.ends_with("e2e-communicator-client.sock"),
        "got {fallback:?}"
    );
}

#[tokio::test]
async fn a_stale_unix_stream_connect_does_not_panic() {
    let path = std::env::temp_dir().join(format!("tui-not-a-socket-{}", uuid::Uuid::new_v4()));
    std::fs::write(&path, b"x").unwrap();

    assert!(UnixStream::connect(&path).await.is_err());

    let (event_tx, mut event_rx) = mpsc::channel(8);
    let (_request_tx, request_rx) = mpsc::channel(8);
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        None,
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    assert!(matches!(
        event_rx.recv().await.unwrap(),
        Incoming::Disconnected { .. }
    ));

    task.abort();
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_facade_that_accepts_and_closes_is_throttled_not_spun() {
    let path = socket_path();
    let listener = UnixListener::bind(&path).unwrap();

    let accepts = Arc::new(AtomicUsize::new(0));
    let server_accepts = accepts.clone();
    let server = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            server_accepts.fetch_add(1, Ordering::SeqCst);
            drop(stream);
        }
    });

    let (event_tx, mut event_rx) = mpsc::channel(4096);
    let (_request_tx, request_rx) = mpsc::channel(32);
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        None,
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    tokio::time::sleep(Duration::from_millis(600)).await;

    let seen = accepts.load(Ordering::SeqCst);
    assert!(
        seen >= 1,
        "the task must try at least once, saw {seen} accepts"
    );
    assert!(
        seen <= 10,
        "an accept-then-close facade must be backed off, not spun: {seen} accepts in 600ms"
    );

    let mut events = 0;
    while event_rx.try_recv().is_ok() {
        events += 1;
    }
    assert!(events > 0, "the attempts must still be reported");

    task.abort();
    server.abort();
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn an_oversized_frame_is_reported_as_an_excerpt_and_ends_the_connection() {
    let path = socket_path();
    let listener = UnixListener::bind(&path).unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read_half, mut write_half) = stream.into_split();
        let mut lines = BufReader::new(read_half).lines();
        lines.next_line().await.unwrap();

        let chunk = vec![b'x'; 64 * 1024];
        for _ in 0..48 {
            if write_half.write_all(&chunk).await.is_err() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    });

    let (event_tx, mut event_rx) = mpsc::channel(32);
    let (_request_tx, request_rx) = mpsc::channel(32);
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        None,
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    let outcome = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match event_rx.recv().await.unwrap() {
                Incoming::Malformed { line } => return line,
                _ => continue,
            }
        }
    })
    .await;

    let line = outcome.expect("an oversized frame must be reported, not buffered forever");
    assert!(
        line.len() < 2000,
        "only an excerpt may reach the app, got {} bytes",
        line.len()
    );
    assert!(
        line.contains(tui::i18n::t("conn.excerpt_more").split('{').next().unwrap()),
        "the excerpt must say how much was dropped: {line}"
    );

    task.abort();
    server.abort();
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn shutdown_flushes_queued_requests_then_closes_the_socket() {
    let path = socket_path();
    let listener = UnixListener::bind(&path).unwrap();
    let (push_tx, push_rx) = mpsc::channel(4);
    let facade = tokio::spawn(fake_facade(listener, push_rx));

    let (event_tx, mut event_rx) = mpsc::channel(32);
    let (request_tx, request_rx) = mpsc::channel(32);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        None,
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    event_rx.recv().await.unwrap();
    event_rx.recv().await.unwrap();

    request_tx
        .send(Request {
            id: 7,
            method: "chats.list".to_string(),
            params: json!({}),
        })
        .await
        .unwrap();
    shutdown_tx.send(true).unwrap();

    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("the connection task must finish on shutdown")
        .unwrap();

    drop(push_tx);
    let seen = tokio::time::timeout(Duration::from_secs(2), facade)
        .await
        .expect("the socket must reach end of stream")
        .unwrap();

    assert_eq!(seen.len(), 2, "handshake plus the queued request: {seen:?}");
    assert_eq!(seen[1]["method"], json!("chats.list"));
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn shutdown_does_not_wait_out_the_reconnect_backoff() {
    let path = socket_path();

    let (event_tx, mut event_rx) = mpsc::channel(32);
    let (_request_tx, request_rx) = mpsc::channel(32);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(connection::run(
        path.clone(),
        None,
        event_tx,
        request_rx,
        shutdown_rx,
    ));

    match event_rx.recv().await.unwrap() {
        Incoming::Disconnected { .. } => {}
        other => panic!("expected a failed connect, got {other:?}"),
    }

    shutdown_tx.send(true).unwrap();

    tokio::time::timeout(Duration::from_millis(200), task)
        .await
        .expect("shutdown must interrupt the backoff")
        .unwrap();
}
