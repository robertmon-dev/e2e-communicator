use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use crate::client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::client::adapters::out::persistance::repositories::outbox::update::Update as OutboxUpdate;
use crate::client::domain::dto::errors::TransportError;
use crate::client::domain::dto::outbox::OutboxEntry;
use crate::client::domain::state::connectivity::ConnectivityState;

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
                let next_retry_at = Utc::now() + backoff(entry.attempts);
                let _ = self
                    .outbox_repository
                    .mark_failed(entry.id, next_retry_at)
                    .await;
            }
        }
    }
}

fn backoff(attempts: i32) -> chrono::Duration {
    let capped = attempts.clamp(0, 8);
    let base_secs = (2i64 << capped).min(300);

    let mut jitter_byte = [0u8; 1];
    let _ = orion::util::secure_rand_bytes(&mut jitter_byte);

    chrono::Duration::seconds(base_secs) + chrono::Duration::milliseconds(jitter_byte[0] as i64 * 4)
}
