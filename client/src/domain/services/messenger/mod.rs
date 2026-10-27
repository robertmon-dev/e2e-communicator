use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::adapters::out::persistance::repositories::message::MessageRepository;
use crate::adapters::out::persistance::repositories::message::create::Create as MessageCreate;
use crate::adapters::out::persistance::repositories::message::remove::Remove;
use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::outbox::create::Create as OutboxCreate;
use common::envelope::{EncryptedEnvelope, EventType};

use crate::domain::dto::errors::{MessageError, MessengerError};
use crate::domain::dto::message::{Attachment, Message, MessageWirePayload};
use crate::domain::dto::outbox::OutboxEntry;
use crate::domain::dto::utils::TimeStep;
use crate::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};
use crate::domain::services::exchanger::Exchanger;

pub struct Messenger {
    message_repository: Arc<MessageRepository>,
    chat_repository: Arc<ChatRepository>,
    outbox_repository: Arc<OutboxRepository>,
    key_repository: Arc<KeyRepository>,
    exchanger: Arc<Exchanger>,
}

impl Messenger {
    pub fn new(
        message_repository: Arc<MessageRepository>,
        chat_repository: Arc<ChatRepository>,
        outbox_repository: Arc<OutboxRepository>,
        key_repository: Arc<KeyRepository>,
        exchanger: Arc<Exchanger>,
    ) -> Self {
        Self {
            message_repository,
            chat_repository,
            outbox_repository,
            key_repository,
            exchanger,
        }
    }

    pub async fn delete(
        &self,
        message_id: Uuid,
        author_id: Uuid,
        author_device_id: Uuid,
    ) -> Result<(), MessengerError> {
        let message = self.message_repository.get(message_id).await?;

        if message.author_id != author_id {
            return Err(MessengerError::Message(MessageError::UnauthorizedAction(
                author_id,
            )));
        }

        let chat = self.chat_repository.get(message.chat_id).await?;
        let latest_state = chat.get_latest()?;

        let wire_payload = serde_json::json!({ "deleted_message_id": message_id });
        let plaintext = serde_json::to_vec(&wire_payload)?;

        let mut envelopes = Vec::new();
        let mut outbox_entries = Vec::new();

        for peer_id in latest_state.members.keys().filter(|id| **id != author_id) {
            let peer_keys = self
                .key_repository
                .list_by_chat_and_user(message.chat_id, *peer_id)
                .await?;

            if peer_keys.is_empty() {
                tracing::warn!(chat_id = %message.chat_id, peer_id = %peer_id, "no known device keys for member, skipping");
                continue;
            }

            for peer_key in peer_keys {
                let shared_key = self
                    .exchanger
                    .derive_shared_key(
                        message.chat_id,
                        (author_id, author_device_id),
                        (*peer_id, peer_key.device_id),
                    )
                    .await?;

                let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext)?;

                let envelope = EncryptedEnvelope {
                    id: Uuid::new_v4(),
                    chat_id: message.chat_id,
                    sender_id: author_id,
                    sender_device_id: author_device_id,
                    recipient_id: *peer_id,
                    recipient_device_id: Some(peer_key.device_id),
                    created_at: Utc::now(),
                    event_type: EventType::MessageDeleted,
                    notify: false,
                    ciphertext,
                };

                outbox_entries.push(OutboxEntry::pending(
                    Some(message_id),
                    *peer_id,
                    &envelope,
                    None,
                ));
                envelopes.push(envelope);
            }
        }

        self.outbox_repository.create_many(outbox_entries).await?;
        self.message_repository.remove_message(message_id).await?;

        Ok(())
    }

    pub async fn mark_delivered(
        &self,
        message_id: Uuid,
        recipient_id: Uuid,
        recipient_device_id: Uuid,
    ) -> Result<(), MessengerError> {
        let message = self.message_repository.get(message_id).await?;

        if message.author_id == recipient_id {
            return Ok(());
        }

        let wire_payload = serde_json::json!({
            "message_id": message_id,
            "delivered_at": Utc::now(),
        });
        let plaintext = serde_json::to_vec(&wire_payload)?;

        let author_keys = self
            .key_repository
            .list_by_chat_and_user(message.chat_id, message.author_id)
            .await?;

        if author_keys.is_empty() {
            tracing::warn!(chat_id = %message.chat_id, author_id = %message.author_id, "no known device keys for message author, skipping delivery receipt");
            return Ok(());
        }

        let mut outbox_entries = Vec::new();

        for author_key in author_keys {
            let shared_key = self
                .exchanger
                .derive_shared_key(
                    message.chat_id,
                    (recipient_id, recipient_device_id),
                    (message.author_id, author_key.device_id),
                )
                .await?;

            let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext)?;

            let envelope = EncryptedEnvelope {
                id: Uuid::new_v4(),
                chat_id: message.chat_id,
                sender_id: recipient_id,
                sender_device_id: recipient_device_id,
                recipient_id: message.author_id,
                recipient_device_id: Some(author_key.device_id),
                created_at: Utc::now(),
                event_type: EventType::DeliveryReceipt,
                notify: false,
                ciphertext,
            };

            outbox_entries.push(OutboxEntry::pending(
                Some(message_id),
                message.author_id,
                &envelope,
                None,
            ));
        }

        self.outbox_repository.create_many(outbox_entries).await?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send(
        &self,
        chat_id: Uuid,
        author_id: Uuid,
        author_device_id: Uuid,
        text: String,
        reply_to_id: Option<Uuid>,
        available_for: Option<TimeStep>,
        attachments: Vec<Attachment>,
        scheduled_for: Option<DateTime<Utc>>,
    ) -> Result<(Message, Vec<EncryptedEnvelope>), MessengerError> {
        if text.trim().is_empty() {
            return Err(MessengerError::Message(MessageError::EmptyContent));
        }

        let message = Message::new(
            author_id,
            chat_id,
            text,
            Vec::new(),
            reply_to_id,
            available_for,
            attachments,
        );

        let (_, latest_content) = message
            .get_latest_with_date()
            .ok_or(MessengerError::Message(MessageError::EmptyHistory))?;

        let message_id = message.id;
        let wire_payload = MessageWirePayload {
            message_id,
            reply_to_id: message.reply_to_id,
            available_to: message.available_to,
            content: latest_content.clone(),
        };
        let plaintext = serde_json::to_vec(&wire_payload)?;

        self.message_repository.create(message).await?;
        let message = self.message_repository.get(message_id).await?;

        let chat = self.chat_repository.get(chat_id).await?;
        let latest_state = chat.get_latest()?;

        let mut envelopes = Vec::new();
        let mut outbox_entries = Vec::new();

        for peer_id in latest_state.members.keys().filter(|id| **id != author_id) {
            let peer_keys = self
                .key_repository
                .list_by_chat_and_user(chat_id, *peer_id)
                .await?;

            if peer_keys.is_empty() {
                tracing::warn!(%chat_id, peer_id = %peer_id, "no known device keys for member, skipping");
                continue;
            }

            for peer_key in peer_keys {
                let shared_key = self
                    .exchanger
                    .derive_shared_key(
                        chat_id,
                        (author_id, author_device_id),
                        (*peer_id, peer_key.device_id),
                    )
                    .await?;

                let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext)?;

                let envelope = EncryptedEnvelope {
                    id: Uuid::new_v4(),
                    chat_id,
                    sender_id: author_id,
                    sender_device_id: author_device_id,
                    recipient_id: *peer_id,
                    recipient_device_id: Some(peer_key.device_id),
                    created_at: Utc::now(),
                    event_type: EventType::Message,
                    notify: true,
                    ciphertext,
                };

                outbox_entries.push(OutboxEntry::pending(
                    Some(message_id),
                    *peer_id,
                    &envelope,
                    scheduled_for,
                ));
                envelopes.push(envelope);
            }
        }

        self.outbox_repository.create_many(outbox_entries).await?;

        Ok((message, envelopes))
    }
}
