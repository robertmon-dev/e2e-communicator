use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio_stream::{Stream, StreamExt};
use uuid::Uuid;

use common::envelope::{EncryptedEnvelope, EventType};

use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::outbox::create::Create as OutboxCreate;
use crate::domain::dto::delivery_receipt::DeliveryReceipt;
use crate::domain::dto::errors::{InboxError, TransportError};
use crate::domain::dto::outbox::OutboxEntry;
use crate::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};
use crate::domain::services::exchanger::Exchanger;
use crate::domain::state::connectivity::ConnectivityState;

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

pub type InboundStream =
    Pin<Box<dyn Stream<Item = Result<(String, EncryptedEnvelope), TransportError>> + Send>>;

pub trait EnvelopeSubscription {
    async fn subscribe(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<InboundStream, TransportError>;

    async fn ack(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        delivery_token: &str,
    ) -> Result<(), TransportError>;
}

pub struct Inbox<T: EnvelopeSubscription> {
    subscription: Arc<T>,
    outbox_repository: Arc<OutboxRepository>,
    exchanger: Arc<Exchanger>,
    connectivity: Arc<ConnectivityState>,
    user_id: Uuid,
    device_id: Uuid,
}

impl<T: EnvelopeSubscription> Inbox<T> {
    pub fn new(
        subscription: Arc<T>,
        outbox_repository: Arc<OutboxRepository>,
        exchanger: Arc<Exchanger>,
        connectivity: Arc<ConnectivityState>,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Self {
        Self {
            subscription,
            outbox_repository,
            exchanger,
            connectivity,
            user_id,
            device_id,
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

            match self
                .subscription
                .subscribe(self.user_id, self.device_id)
                .await
            {
                Ok(mut stream) => {
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok((delivery_token, envelope)) => {
                                self.handle(delivery_token, envelope).await;
                            }
                            Err(_) => break,
                        }
                    }
                }
                Err(_) => tokio::time::sleep(RECONNECT_DELAY).await,
            }
        }
    }

    async fn handle(&self, delivery_token: String, envelope: EncryptedEnvelope) {
        let _ = self
            .subscription
            .ack(self.user_id, self.device_id, &delivery_token)
            .await;

        if matches!(
            envelope.event_type,
            EventType::Message | EventType::Reaction
        ) {
            if let Err(e) = self.send_delivery_receipt(&envelope).await {
                tracing::warn!("Failed to enqueue delivery receipt: {e}");
            }
        }
    }

    async fn send_delivery_receipt(&self, envelope: &EncryptedEnvelope) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(envelope.chat_id, self.user_id, envelope.sender_id)
            .await?;

        let receipt = DeliveryReceipt {
            id: Uuid::new_v4(),
            message_id: envelope.id,
            chat_id: envelope.chat_id,
            user_id: self.user_id,
        };

        let plaintext = serde_json::to_vec(&receipt)?;
        let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext)?;

        let receipt_envelope = EncryptedEnvelope {
            id: Uuid::new_v4(),
            chat_id: envelope.chat_id,
            sender_id: self.user_id,
            recipient_id: envelope.sender_id,
            created_at: Utc::now(),
            event_type: EventType::DeliveryReceipt,
            ciphertext,
        };

        let outbox_entry =
            OutboxEntry::pending(receipt.id, envelope.sender_id, &receipt_envelope, None);

        self.outbox_repository
            .create_many(vec![outbox_entry])
            .await?;

        Ok(())
    }
}
