mod fixtures;

use redis::AsyncCommands;
use tokio_stream::StreamExt;

#[tokio::test]
async fn pubsub_connection_receives_published_messages() {
    let store = fixtures::store();

    let mut pubsub = store.pubsub_connection().await.unwrap();
    pubsub.subscribe("test:pubsub-connection").await.unwrap();
    let mut stream = pubsub.on_message();

    let mut publisher = store.connection().await.unwrap();
    let _: usize = publisher
        .publish("test:pubsub-connection", "hello")
        .await
        .unwrap();

    let msg = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .expect("timed out waiting for published message")
        .expect("pubsub stream ended unexpectedly");

    let payload: String = msg.get_payload().unwrap();
    assert_eq!(payload, "hello");
}

#[test]
fn needs_reconnect_flags_failover_replies_but_not_ordinary_errors() {
    use redis::{ErrorKind, RedisError, ServerErrorKind};
    use server::core::initializers::redis::MailboxStore;

    let server_error = |kind, code: &'static str, detail: &str| -> RedisError {
        (ErrorKind::Server(kind), code, detail.to_string()).into()
    };

    let readonly = server_error(
        ServerErrorKind::ReadOnly,
        "READONLY",
        "You can't write against a read only replica.",
    );
    let master_down = server_error(
        ServerErrorKind::MasterDown,
        "MASTERDOWN",
        "Link with MASTER is down.",
    );
    let loading = server_error(
        ServerErrorKind::BusyLoading,
        "LOADING",
        "Redis is loading the dataset in memory",
    );
    let response = server_error(ServerErrorKind::ResponseError, "ERR", "unknown command");

    assert_eq!(readonly.code(), Some("READONLY"));
    assert!(MailboxStore::needs_reconnect(&readonly));
    assert!(MailboxStore::needs_reconnect(&master_down));
    assert!(MailboxStore::needs_reconnect(&loading));
    assert!(!MailboxStore::needs_reconnect(&response));
}

#[tokio::test]
async fn blocking_connection_hands_out_a_fresh_socket_each_time() {
    let store = fixtures::store();

    let client_id = |mut conn: redis::aio::MultiplexedConnection| async move {
        redis::cmd("CLIENT")
            .arg("ID")
            .query_async::<i64>(&mut conn)
            .await
            .unwrap()
    };

    let first = client_id(store.blocking_connection().await.unwrap()).await;
    let second = client_id(store.blocking_connection().await.unwrap()).await;
    let cached = client_id(store.connection().await.unwrap()).await;
    let cached_again = client_id(store.connection().await.unwrap()).await;

    assert_ne!(first, second, "blocking connections must not be shared");
    assert_ne!(first, cached);
    assert_ne!(second, cached);
    assert_eq!(cached, cached_again, "the shared connection stays cached");
}

#[tokio::test]
async fn a_blocking_read_does_not_stall_commands_on_the_shared_connection() {
    let store = fixtures::store();
    let stream_key = format!("test:blocking:{}", uuid::Uuid::new_v4());

    let mut blocking = store.blocking_connection().await.unwrap();
    let blocked = tokio::spawn({
        let stream_key = stream_key.clone();
        async move {
            let opts = redis::streams::StreamReadOptions::default().block(2000);
            let _: Option<redis::streams::StreamReadReply> = blocking
                .xread_options(&[stream_key.as_str()], &["$"], &opts)
                .await
                .unwrap();
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let started = std::time::Instant::now();
    let pong: String = store
        .run(|mut conn| async move { redis::cmd("PING").query_async(&mut conn).await })
        .await
        .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(pong, "PONG");
    assert!(
        elapsed < std::time::Duration::from_millis(200),
        "PING waited {elapsed:?} behind the blocking read"
    );

    blocked.await.unwrap();
}
