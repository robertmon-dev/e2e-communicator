use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio_stream::{Stream, StreamExt};
use uuid::Uuid;

use common::envelope::{EncryptedEnvelope, EventType};

use crate::adapters::out::persistance::repositories::message::MessageRepository;
use crate::adapters::out::persistance::repositories::message::create::Create as MessageCreate;
use crate::adapters::out::persistance::repositories::message::remove::Remove;
use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::outbox::create::Create as OutboxCreate;
use crate::adapters::out::persistance::repositories::outbox::update::Update as OutboxUpdate;
use crate::domain::dto::delivery_receipt::DeliveryReceipt;
use crate::domain::dto::errors::{InboxError, TransportError};
use crate::domain::dto::message::{Message, MessageDeletedWirePayload, MessageWirePayload};
use crate::domain::dto::outbox::OutboxEntry;
use crate::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};
use crate::domain::services::exchanger::Exchanger;
use crate::domain::state::connectivity::ConnectivityState;
use crate::domain::state::facade_events::{FacadeEvent, FacadeEventBus};

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
    message_repository: Arc<MessageRepository>,
    outbox_repository: Arc<OutboxRepository>,
    exchanger: Arc<Exchanger>,
    connectivity: Arc<ConnectivityState>,
    user_id: Uuid,
    device_id: Uuid,
    events: FacadeEventBus,
}

impl<T: EnvelopeSubscription> Inbox<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        subscription: Arc<T>,
        message_repository: Arc<MessageRepository>,
        outbox_repository: Arc<OutboxRepository>,
        exchanger: Arc<Exchanger>,
        connectivity: Arc<ConnectivityState>,
        user_id: Uuid,
        device_id: Uuid,
        events: FacadeEventBus,
    ) -> Self {
        Self {
            subscription,
            message_repository,
            outbox_repository,
            exchanger,
            connectivity,
            user_id,
            device_id,
            events,
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

    pub async fn handle(&self, delivery_token: String, envelope: EncryptedEnvelope) {
        let _ = self
            .subscription
            .ack(self.user_id, self.device_id, &delivery_token)
            .await;

        let result = match envelope.event_type.clone() {
            EventType::Message => self.handle_message(&envelope).await,
            EventType::DeliveryReceipt => self.handle_delivery_receipt(&envelope).await,
            EventType::MessageDeleted => self.handle_message_deleted(&envelope).await,
            EventType::Reaction
            | EventType::ReadReceipt
            | EventType::OverlayReceipt
            | EventType::InvitationReceipt => Ok(()),
        };

        if let Err(e) = result {
            tracing::warn!("Failed to handle inbound envelope {}: {e}", envelope.id);
        }
    }

    async fn handle_message(&self, envelope: &EncryptedEnvelope) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(envelope.chat_id, self.user_id, envelope.sender_id)
            .await?;

        let plaintext = Encryptor::decrypt_with_derived_key(&shared_key, &envelope.ciphertext)?;
        let payload: MessageWirePayload = serde_json::from_slice(&plaintext)?;

        let message_id = payload.message_id;
        let reply_to_id = payload.reply_to_id;
        let available_to = payload.available_to;

        let mut history = BTreeMap::new();
        history.insert(envelope.created_at, payload.content);

        let message = Message {
            id: message_id,
            author_id: envelope.sender_id,
            chat_id: envelope.chat_id,
            history,
            seen_by: BTreeMap::new(),
            created_at: envelope.created_at,
            updated_at: envelope.created_at,
            available_to,
            hidden: false,
            is_reply: reply_to_id.is_some(),
            reply_to_id,
        };

        self.message_repository.upsert(message.clone()).await?;
        let _ = self.events.send(FacadeEvent::MessageReceived(message));

        self.send_delivery_receipt(envelope, message_id).await
    }

    async fn handle_message_deleted(&self, envelope: &EncryptedEnvelope) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(envelope.chat_id, self.user_id, envelope.sender_id)
            .await?;

        let plaintext = Encryptor::decrypt_with_derived_key(&shared_key, &envelope.ciphertext)?;
        let payload: MessageDeletedWirePayload = serde_json::from_slice(&plaintext)?;

        self.message_repository
            .remove_message(payload.deleted_message_id)
            .await?;
        self.send_delivery_receipt(envelope, payload.deleted_message_id)
            .await
    }

    async fn handle_delivery_receipt(
        &self,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(envelope.chat_id, self.user_id, envelope.sender_id)
            .await?;

        let plaintext = Encryptor::decrypt_with_derived_key(&shared_key, &envelope.ciphertext)?;
        let receipt: DeliveryReceipt = serde_json::from_slice(&plaintext)?;

        let entry = self
            .outbox_repository
            .get_by_message_and_recipient(receipt.message_id, envelope.sender_id)
            .await?;

        self.outbox_repository.mark_delivered(entry.id).await?;
        let _ = self.events.send(FacadeEvent::DeliveryReceiptMarked {
            message_id: receipt.message_id,
            recipient_id: envelope.sender_id,
        });

        Ok(())
    }

    async fn send_delivery_receipt(
        &self,
        envelope: &EncryptedEnvelope,
        message_id: Uuid,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(envelope.chat_id, self.user_id, envelope.sender_id)
            .await?;

        let receipt = DeliveryReceipt {
            id: Uuid::new_v4(),
            message_id,
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
            OutboxEntry::pending(message_id, envelope.sender_id, &receipt_envelope, None);

        self.outbox_repository
            .create_many(vec![outbox_entry])
            .await?;

        Ok(())
    }
}
