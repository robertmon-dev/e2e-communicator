use std::sync::Arc;
use std::time::Duration;

use redis::AsyncConnectionConfig;
use redis::ConnectionAddr;
use redis::aio::MultiplexedConnection;
use redis::sentinel::{SentinelClient, SentinelClientBuilder, SentinelServerType};
use tokio::sync::Mutex;

use crate::core::initializers::settings::RedisSettings;
use crate::domain::dto::errors::MailboxError;

const RESPONSE_TIMEOUT: Duration = Duration::from_millis(6_000);

pub struct MailboxStore {
    sentinel: Mutex<SentinelClient>,
}

impl MailboxStore {
    pub fn new(settings: &RedisSettings) -> Result<Arc<Self>, MailboxError> {
        let sentinel_nodes = settings
            .sentinel_nodes
            .iter()
            .map(|addr| parse_tcp_addr(addr))
            .collect::<Result<Vec<ConnectionAddr>, MailboxError>>()?;

        let mut builder = SentinelClientBuilder::new(
            sentinel_nodes,
            settings.service_name.clone(),
            SentinelServerType::Master,
        )
        .map_err(MailboxError::from)?;

        if let Some(password) = &settings.password {
            builder = builder.set_client_to_redis_password(password);
        }

        let sentinel_client = builder.build()?;

        Ok(Arc::new(Self {
            sentinel: Mutex::new(sentinel_client),
        }))
    }

    pub async fn connection(&self) -> Result<MultiplexedConnection, MailboxError> {
        let mut guard = self.sentinel.lock().await;
        let config = AsyncConnectionConfig::new().set_response_timeout(Some(RESPONSE_TIMEOUT));
        let connection = guard
            .get_async_connection_with_config(&config)
            .await
            .inspect_err(|err| {
                tracing::error!(error = %err, "failed to resolve master via Sentinel");
            })?;

        Ok(connection)
    }

    pub async fn pubsub_connection(&self) -> Result<redis::aio::PubSub, MailboxError> {
        let mut guard = self.sentinel.lock().await;
        let client = guard.async_get_client().await.inspect_err(|err| {
            tracing::error!(error = %err, "failed to resolve master via Sentinel for pubsub");
        })?;

        let pubsub = client.get_async_pubsub().await?;
        Ok(pubsub)
    }
}

fn parse_tcp_addr(addr: &str) -> Result<ConnectionAddr, MailboxError> {
    let (host, port) = addr
        .rsplit_once(':')
        .ok_or_else(|| MailboxError::InvalidConfig(format!("invalid sentinel address: {addr}")))?;

    let port: u16 = port
        .parse()
        .map_err(|_| MailboxError::InvalidConfig(format!("invalid sentinel port in: {addr}")))?;

    Ok(ConnectionAddr::Tcp(host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use redis::AsyncCommands;
    use tokio_stream::StreamExt;

    use super::*;

    fn test_settings() -> RedisSettings {
        RedisSettings {
            sentinel_nodes: vec![
                "localhost:26379".to_string(),
                "localhost:26380".to_string(),
                "localhost:26381".to_string(),
            ],
            service_name: "mymaster".to_string(),
            password: None,
        }
    }

    #[tokio::test]
    async fn pubsub_connection_receives_published_messages() {
        let store = MailboxStore::new(&test_settings())
            .expect("failed to build MailboxStore - is `make redis-up` running?");

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
}
