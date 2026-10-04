use std::sync::Arc;

use redis::ConnectionAddr;
use redis::aio::MultiplexedConnection;
use redis::sentinel::{SentinelClient, SentinelClientBuilder, SentinelServerType};
use tokio::sync::Mutex;

use crate::core::initializers::settings::RedisSettings;
use crate::domain::dto::errors::MailboxError;

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
        let connection = guard.get_async_connection().await.inspect_err(|err| {
            tracing::error!(error = %err, "failed to resolve master via Sentinel");
        })?;

        Ok(connection)
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
