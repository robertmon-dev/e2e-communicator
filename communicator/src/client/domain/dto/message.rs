use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::BTreeMap;
use url::Url;
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::{
    attachment, file_metadata, message, message_history,
};
use crate::client::domain::dto::utils::TimeStep;
use crate::client::domain::dto::{errors::MessageError, errors::ValueError, reaction::Reaction};

use super::utils::FileMetadata;

#[derive(Debug, Clone, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(i32)]
pub enum AttachmentType {
    File = 0,
    Image = 1,
    Video = 2,
}

impl AttachmentType {
    pub fn from(num: i32) -> Result<Self, ValueError> {
        match num {
            0 => Ok(Self::File),
            1 => Ok(Self::Image),
            2 => Ok(Self::Video),
            n if n < 0 => Err(ValueError::LesserThanExpected),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Attachment {
    pub id: Uuid,

    pub chat_id: Uuid,

    pub author_id: Uuid,
    pub message_history_id: Uuid,

    pub attachment_link: Option<Url>,
    pub attachment_type: AttachmentType,

    pub file_metadata: FileMetadata,
}

impl Attachment {
    pub fn new(
        id: Uuid,
        chat_id: Uuid,
        author_id: Uuid,
        message_history_id: Uuid,
        attachment_link: Option<Url>,
        attachment_type: AttachmentType,
        file_metadata: FileMetadata,
    ) -> Self {
        Self {
            id,
            chat_id,
            author_id,
            message_history_id,
            attachment_link,
            attachment_type,
            file_metadata,
        }
    }

    pub fn from(
        attachment_model: attachment::Model,
        file_metadata: file_metadata::Model,
    ) -> Result<Self, ValueError> {
        let file_metadata_transferred = FileMetadata::new(
            file_metadata.id,
            file_metadata.file_name,
            file_metadata.file_size_bytes,
            file_metadata.mime_type,
            file_metadata.width,
            file_metadata.height,
            file_metadata.duration_secs,
            file_metadata.thumbnail_link,
            file_metadata.created_at,
            file_metadata.updated_at,
        );

        let attachment_type = AttachmentType::from(attachment_model.attachment_type)?;
        if let Some(attachment_link) = attachment_model.attachment_link {
            let attachment_link = Url::parse(attachment_link.as_str())?;

            Ok(Self {
                id: attachment_model.id,
                chat_id: attachment_model.chat_id,
                author_id: attachment_model.author_id,
                message_history_id: attachment_model.message_history_id,
                attachment_link: Some(attachment_link),
                attachment_type,
                file_metadata: file_metadata_transferred,
            })
        } else {
            Err(ValueError::AttachmentWithoutAccessLink)
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageContent {
    pub text: String,
    pub attachments: Option<Vec<Attachment>>,
    pub reactions: Option<Vec<Reaction>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub id: Uuid,
    pub author_id: Uuid,
    pub chat_id: Uuid,

    pub history: BTreeMap<DateTime<Utc>, MessageContent>,
    pub seen_by: BTreeMap<Uuid, DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub available_to: Option<DateTime<Utc>>,
    pub hidden: bool,

    pub is_reply: bool,
    pub reply_to_id: Option<Uuid>,
}

pub type UnstructurizedMessageStory = Vec<(
    message_history::Model,
    (Vec<attachment::Model>, Vec<file_metadata::Model>),
)>;
pub type UnstructurizedMessage = (message::Model, UnstructurizedMessageStory);

impl Message {
    pub fn from(
        message: message::Model,
        history: UnstructurizedMessageStory,
    ) -> Result<Self, ValueError> {
        let mut transferred_history: BTreeMap<DateTime<Utc>, MessageContent> = BTreeMap::new();
        let seen_by: BTreeMap<Uuid, DateTime<Utc>> = BTreeMap::new();

        for (history, (attachment_models, file_metadata_models)) in history {
            let attachments_transferred = attachment_models
                .into_iter()
                .zip(file_metadata_models.into_iter())
                .map(|(attachment_model, file_metadata_model)| {
                    Attachment::from(attachment_model, file_metadata_model)
                })
                .collect::<Result<Vec<Attachment>, _>>()?;

            let message_content = MessageContent {
                text: history.text,
                attachments: if attachments_transferred.is_empty() {
                    None
                } else {
                    Some(attachments_transferred)
                },
                reactions: None,
            };

            transferred_history.insert(history.created_at, message_content);
        }

        Ok(Self {
            id: message.id,
            author_id: message.author_id,
            chat_id: message.chat_id,
            created_at: message.created_at,
            available_to: message.available_to,
            hidden: message.hidden,
            is_reply: message.is_reply,
            reply_to_id: message.reply_to_id,
            history: transferred_history,
            seen_by,
        })
    }

    pub fn new(
        author_id: Uuid,
        chat_id: Uuid,
        initial_message: String,
        reactions: Option<Vec<Reaction>>,
        reply_to_id: Option<Uuid>,
        available_for: Option<TimeStep>,
        attachments: Option<Vec<Attachment>>,
    ) -> Self {
        let now = Utc::now();
        let mut history = BTreeMap::new();

        let message_content = MessageContent {
            text: initial_message,
            attachments,
            reactions,
        };

        history.insert(now, message_content);
        let available_to = available_for.map(|available| now + available.as_duration());

        Self {
            id: Uuid::new_v4(),
            chat_id,
            author_id,
            created_at: now,
            available_to,
            history,
            seen_by: BTreeMap::new(),

            hidden: false,

            is_reply: reply_to_id.is_some(),
            reply_to_id,
        }
    }

    pub fn get_latest_with_date(&self) -> Option<(&DateTime<Utc>, &MessageContent)> {
        self.history.iter().last()
    }

    pub fn add_reaction(&mut self, reaction: Reaction) -> &mut Self {
        let now = Utc::now();

        if let Some(latest_content) = self.history.values().last() {
            let mut updated_reactions = latest_content.reactions.clone().unwrap_or_default();

            updated_reactions.push(reaction);

            let new_content = MessageContent {
                text: latest_content.text.clone(),
                attachments: latest_content.attachments.clone(),
                reactions: Some(updated_reactions),
            };

            self.history.insert(now, new_content);
        }

        self
    }

    pub fn remove_reaction(&mut self, reaction_id: Uuid) -> Result<&mut Self, MessageError> {
        let now = Utc::now();

        let latest_content = self
            .history
            .values()
            .last()
            .ok_or(MessageError::EmptyHistory)?;

        if let Some(reactions) = latest_content.reactions.as_ref()
            && let Some(index) = reactions.iter().position(|r| r.id == reaction_id)
        {
            let mut updated_reactions = reactions.clone();
            updated_reactions.remove(index);

            let new_reactions = if updated_reactions.is_empty() {
                None
            } else {
                Some(updated_reactions)
            };

            let new_content = MessageContent {
                text: latest_content.text.clone(),
                attachments: latest_content.attachments.clone(),
                reactions: new_reactions,
            };

            self.history.insert(now, new_content);

            return Ok(self);
        }

        Err(MessageError::ReactionNotFound(reaction_id))
    }
}
