use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::client::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::client::adapters::out::persistance::repositories::message::MessageRepository;
use crate::client::adapters::out::persistance::repositories::message::create::Create;
use crate::client::domain::dto::envelope::{EncryptedEnvelope, EventType};
use crate::client::domain::dto::errors::{MessageError, MessengerError};
use crate::client::domain::dto::message::{Attachment, Message};
use crate::client::domain::dto::utils::TimeStep;
use crate::client::domain::services::encryptor::Encryptor;
use crate::client::domain::services::exchanger::Exchanger;

pub struct Messenger {
    message_repository: Arc<MessageRepository>,
    chat_repository: Arc<ChatRepository>,
    exchanger: Arc<Exchanger>,
}

impl Messenger {
    pub fn new(
        message_repository: Arc<MessageRepository>,
        chat_repository: Arc<ChatRepository>,
        exchanger: Arc<Exchanger>,
    ) -> Self {
        Self {
            message_repository,
            chat_repository,
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

        for peer_id in latest_state.members.keys().filter(|id| **id != author_id) {
            let shared_key = self
                .exchanger
                .derive_shared_key(chat_id, author_id, *peer_id)
                .await?;

            let ciphertext = Encryptor::seal(&shared_key, &plaintext)?;

            envelopes.push(EncryptedEnvelope {
                id: Uuid::new_v4(),
                chat_id,
                sender_id: author_id,
                created_at: Utc::now(),
                event_type: EventType::Message,
                ciphertext,
            });
        }

        Ok((message, envelopes))
    }
}
