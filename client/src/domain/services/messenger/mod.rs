use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::adapters::out::persistance::repositories::message::MessageRepository;
use crate::adapters::out::persistance::repositories::message::create::Create as MessageCreate;
use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::outbox::create::Create as OutboxCreate;
use common::envelope::{EncryptedEnvelope, EventType};

use crate::domain::dto::errors::{MessageError, MessengerError};
use crate::domain::dto::message::{Attachment, Message};
use crate::domain::dto::outbox::OutboxEntry;
use crate::domain::dto::utils::TimeStep;
use crate::domain::services::encryptor::Encryptor;
use crate::domain::services::exchanger::Exchanger;

pub struct Messenger {
    message_repository: Arc<MessageRepository>,
    chat_repository: Arc<ChatRepository>,
    outbox_repository: Arc<OutboxRepository>,
    exchanger: Arc<Exchanger>,
}

impl Messenger {
    pub fn new(
        message_repository: Arc<MessageRepository>,
        chat_repository: Arc<ChatRepository>,
        outbox_repository: Arc<OutboxRepository>,
        exchanger: Arc<Exchanger>,
    ) -> Self {
        Self {
            message_repository,
            chat_repository,
            outbox_repository,
            exchanger,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send(
        &self,
        chat_id: Uuid,
        author_id: Uuid,
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
        let plaintext = serde_json::to_vec(latest_content)?;
        let message_id = message.id;

        self.message_repository.create(message).await?;
        let message = self.message_repository.get(message_id).await?;

        let chat = self.chat_repository.get(chat_id).await?;
        let latest_state = chat.get_latest()?;

        let mut envelopes = Vec::new();
        let mut outbox_entries = Vec::new();

        for peer_id in latest_state.members.keys().filter(|id| **id != author_id) {
            let shared_key = self
                .exchanger
                .derive_shared_key(chat_id, author_id, *peer_id)
                .await?;

            let ciphertext = Encryptor::seal(&shared_key, &plaintext)?;

            let envelope = EncryptedEnvelope {
                id: Uuid::new_v4(),
                chat_id,
                sender_id: author_id,
                recipient_id: *peer_id,
                created_at: Utc::now(),
                event_type: EventType::Message,
                ciphertext,
            };

            outbox_entries.push(OutboxEntry::pending(
                message_id,
                *peer_id,
                &envelope,
                scheduled_for,
            ));
            envelopes.push(envelope);
        }

        self.outbox_repository.create_many(outbox_entries).await?;

        Ok((message, envelopes))
    }
}
