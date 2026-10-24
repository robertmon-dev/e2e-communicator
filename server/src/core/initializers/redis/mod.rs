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
