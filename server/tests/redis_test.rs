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
