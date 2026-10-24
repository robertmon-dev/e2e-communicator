use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch::Receiver;

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
use crate::domain::state::identity::{IdentityState, IdentityStatus};
use crate::domain::state::session::SessionState;

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
    identity: Arc<IdentityState>,
    session: Arc<SessionState>,
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
        identity: Arc<IdentityState>,
        session: Arc<SessionState>,
        events: FacadeEventBus,
    ) -> Self {
        Self {
            subscription,
            message_repository,
            outbox_repository,
            exchanger,
            connectivity,
            identity,
            session,
            events,
        }
    }

    pub async fn run(&self, _shutdown: Receiver<bool>) -> ! {
        loop {
            if !self.connectivity.is_online() {
                let mut receiver = self.connectivity.subscribe();
                while !self.connectivity.is_online() {
                    let _ = receiver.changed().await;
                }
                continue;
            }

            let Some(local) = self.identity.registered() else {
                let mut receiver = self.identity.subscribe();
                let _ = receiver
                    .wait_for(|status| matches!(status, IdentityStatus::Registered(_)))
                    .await;
                continue;
            };

            if self.session.get().is_none() {
                let mut receiver = self.session.subscribe();
                let _ = receiver.wait_for(|session| session.is_some()).await;
                continue;
            }

            match self
                .subscription
                .subscribe(local.user_id, local.device_id)
                .await
            {
                Ok(mut stream) => {
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok((delivery_token, envelope)) => {
                                self.handle(delivery_token, envelope).await;
                            }
                            Err(TransportError::Unauthenticated(reason)) => {
                                tracing::warn!(%reason, "Inbox stream rejected as unauthenticated");
                                self.session.clear();
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                }
                Err(TransportError::Unauthenticated(reason)) => {
                    tracing::warn!(%reason, "Inbox subscribe rejected as unauthenticated");
                    self.session.clear();
                }
                Err(_) => tokio::time::sleep(RECONNECT_DELAY).await,
            }
        }
    }

    pub async fn handle(&self, delivery_token: String, envelope: EncryptedEnvelope) {
        let Some(local) = self.identity.registered() else {
            tracing::warn!(
                envelope_id = %envelope.id,
                "Inbound envelope while unregistered, ignoring"
            );
            return;
        };

        let _ = self
            .subscription
            .ack(local.user_id, local.device_id, &delivery_token)
            .await;

        let result = match envelope.event_type.clone() {
            EventType::Message => self.handle_message(local.user_id, &envelope).await,
            EventType::DeliveryReceipt => {
                self.handle_delivery_receipt(local.user_id, &envelope).await
            }
            EventType::MessageDeleted => {
                self.handle_message_deleted(local.user_id, &envelope).await
            }
            EventType::Reaction
            | EventType::ReadReceipt
            | EventType::OverlayReceipt
            | EventType::InvitationReceipt => Ok(()),
        };

        if let Err(e) = result {
            tracing::warn!("Failed to handle inbound envelope {}: {e}", envelope.id);
        }
    }

    async fn handle_message(
        &self,
        local_user_id: Uuid,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(envelope.chat_id, local_user_id, envelope.sender_id)
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

        self.send_delivery_receipt(local_user_id, envelope, message_id)
            .await
    }

    async fn handle_message_deleted(
        &self,
        local_user_id: Uuid,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(envelope.chat_id, local_user_id, envelope.sender_id)
            .await?;

        let plaintext = Encryptor::decrypt_with_derived_key(&shared_key, &envelope.ciphertext)?;
        let payload: MessageDeletedWirePayload = serde_json::from_slice(&plaintext)?;

        self.message_repository
            .remove_message(payload.deleted_message_id)
            .await?;
        self.send_delivery_receipt(local_user_id, envelope, payload.deleted_message_id)
            .await
    }

    async fn handle_delivery_receipt(
        &self,
        local_user_id: Uuid,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(envelope.chat_id, local_user_id, envelope.sender_id)
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
        local_user_id: Uuid,
        envelope: &EncryptedEnvelope,
        message_id: Uuid,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(envelope.chat_id, local_user_id, envelope.sender_id)
            .await?;

        let receipt = DeliveryReceipt {
            id: Uuid::new_v4(),
            message_id,
            chat_id: envelope.chat_id,
            user_id: local_user_id,
        };

        let plaintext = serde_json::to_vec(&receipt)?;
        let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext)?;

        let receipt_envelope = EncryptedEnvelope {
            id: Uuid::new_v4(),
            chat_id: envelope.chat_id,
            sender_id: local_user_id,
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
