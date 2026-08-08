use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::outbox::update::Update as OutboxUpdate;
use crate::domain::dto::errors::TransportError;
use crate::domain::dto::outbox::OutboxEntry;
use crate::domain::state::connectivity::ConnectivityState;

const DEFAULT_BATCH_SIZE: u64 = 20;
const DEFAULT_IDLE_INTERVAL: Duration = Duration::from_secs(5);

pub trait EnvelopeTransport {
    async fn send(&self, entry: &OutboxEntry) -> Result<(), TransportError>;
}

pub struct Relay<T: EnvelopeTransport> {
    outbox_repository: Arc<OutboxRepository>,
    connectivity: Arc<ConnectivityState>,
    transport: Arc<T>,
    batch_size: u64,
    idle_interval: Duration,
}

impl<T: EnvelopeTransport> Relay<T> {
    pub fn new(
        outbox_repository: Arc<OutboxRepository>,
        connectivity: Arc<ConnectivityState>,
        transport: Arc<T>,
    ) -> Self {
        Self {
            outbox_repository,
            connectivity,
            transport,
            batch_size: DEFAULT_BATCH_SIZE,
            idle_interval: DEFAULT_IDLE_INTERVAL,
        }
    }

    pub async fn run(&self) -> ! {
        loop {
            if !self.connectivity.is_online() {
                let mut receiver = self.connectivity.subscribe();
                while !self.connectivity.is_online() {
                    let _ = receiver.changed().await;
                }
                continue;
            }

            match self.outbox_repository.list_due(self.batch_size).await {
                Ok(entries) if !entries.is_empty() => {
                    for entry in entries {
                        self.attempt(entry).await;
                    }
                }
                _ => tokio::time::sleep(self.idle_interval).await,
            }
        }
    }

    async fn attempt(&self, entry: OutboxEntry) {
        match self.transport.send(&entry).await {
            Ok(()) => {
                let _ = self.outbox_repository.mark_sent(entry.id).await;
            }
            Err(_) => {
                let next_retry_at = Utc::now()
                    + chrono::Duration::from_std(common::backoff::exponential_with_jitter(
                        entry.attempts,
                        Duration::from_secs(300),
                    ))
                    .unwrap_or_else(|_| chrono::Duration::seconds(300));
                let _ = self
                    .outbox_repository
                    .mark_failed(entry.id, next_retry_at)
                    .await;
            }
        }
    }
}
