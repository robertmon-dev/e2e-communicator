use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::BTreeMap;
use std::convert::TryFrom;
use url::Url;
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::{
    attachment, file_metadata, message, message_history, reaction, reaction_history,
};
use crate::client::domain::dto::utils::TimeStep;
use crate::client::domain::dto::{errors::MessageError, errors::ValueError, reaction::Reaction};

use super::utils::FileMetadata;
use utoipa::ToSchema;

#[derive(Debug, Copy, Clone, PartialEq, Serialize_repr, Deserialize_repr, ToSchema)]
#[repr(i32)]
pub enum AttachmentType {
    File = 0,
    Image = 1,
    Video = 2,
    Unknown = 3,
}

impl TryFrom<i32> for AttachmentType {
    type Error = ValueError;

    fn try_from(i: i32) -> Result<Self, ValueError> {
        match i {
            0 => Ok(Self::File),
            1 => Ok(Self::Image),
            2 => Ok(Self::Video),
            3 => Ok(Self::Unknown),
            n if n < 0 => Err(ValueError::LesserThanExpected),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

impl From<AttachmentType> for i32 {
    fn from(i: AttachmentType) -> Self {
        match i {
            AttachmentType::File => 0,
            AttachmentType::Image => 1,
            AttachmentType::Video => 2,
            AttachmentType::Unknown => 3,
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

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    pub available_to: Option<DateTime<Utc>>,

    pub views_left: Option<i32>,
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
        created_at: DateTime<Utc>,
        available_to: Option<DateTime<Utc>>,
        views: Option<i32>,
    ) -> Self {
        let now = Utc::now();

        Self {
            id,
            chat_id,
            author_id,
            message_history_id,
            attachment_link,
            attachment_type,
            file_metadata,
            created_at,
            updated_at: now,
            available_to,
            views_left: views,
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
        );

        let attachment_type = AttachmentType::try_from(attachment_model.attachment_type)?;
        let attachment_link = attachment_model
            .attachment_link
            .map(|link| Url::parse(link.as_str()))
            .transpose()?;

        Ok(Self {
            id: attachment_model.id,
            chat_id: attachment_model.chat_id,
            author_id: attachment_model.author_id,
            message_history_id: attachment_model.message_history_id,
            attachment_link,
            attachment_type,
            file_metadata: file_metadata_transferred,
            created_at: attachment_model.created_at,
            updated_at: attachment_model.updated_at,
            available_to: attachment_model.available_to,
            views_left: attachment_model.views_left,
        })
    }

    pub fn into_model(&self) -> attachment::Model {
        let attachment_link = self
            .attachment_link
            .as_ref()
            .map(|attachment_url| attachment_url.as_str().to_string());

        attachment::Model {
            id: self.id,
            chat_id: self.chat_id,
            author_id: self.author_id,
            message_history_id: self.message_history_id,
            file_metadata_id: self.file_metadata.id,
            attachment_link,
            attachment_type: self.attachment_type.into(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            available_to: self.available_to,
            views_left: self.views_left,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageContent {
    pub id: Uuid,
    pub text: String,
    pub attachments: Vec<Attachment>,
    pub reactions: Vec<Reaction>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub id: Uuid,
    pub author_id: Uuid,
    pub chat_id: Uuid,

    pub history: BTreeMap<DateTime<Utc>, MessageContent>,
    pub seen_by: BTreeMap<Uuid, DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    pub available_to: Option<DateTime<Utc>>,
    pub hidden: bool,

    pub is_reply: bool,
    pub reply_to_id: Option<Uuid>,
}

pub type UnstructurizedMessageStory = Vec<(
    message_history::Model,
    (Vec<attachment::Model>, Vec<file_metadata::Model>),
    Vec<(reaction::Model, Vec<reaction_history::Model>)>,
)>;
pub type UnstructurizedMessage = (message::Model, UnstructurizedMessageStory);

impl From<Message> for UnstructurizedMessage {
    fn from(message: Message) -> Self {
        let message_model = message::Model {
            id: message.id,
            author_id: message.author_id,
            chat_id: message.chat_id,
            reply_to_id: message.reply_to_id,
            available_to: message.available_to,
            hidden: message.hidden,
            is_reply: message.is_reply,
            updated_at: message.updated_at,
            created_at: message.created_at,
        };

        let mut message_history_vec: UnstructurizedMessageStory = Vec::new();

        for (time_entry, message_content) in message.history.into_iter() {
            let history_model = message_history::Model {
                id: message_content.id,
                text: message_content.text,
                message_id: message.id,
                created_at: time_entry,
            };
            let mut attachment_models = Vec::new();
            let mut file_metadata_models = Vec::new();

            for att in message_content.attachments.iter() {
                file_metadata_models.push(att.file_metadata.into_model());
                attachment_models.push(att.into_model());
            }

            let mut reaction_models_vec = Vec::new();

            for reaction in message_content.reactions.iter() {
                reaction_models_vec.push(reaction.into());
            }

            message_history_vec.push((
                history_model,
                (attachment_models, file_metadata_models),
                reaction_models_vec,
            ));
        }

        (message_model, message_history_vec)
    }
}

impl Message {
    pub fn from(
        message: message::Model,
        history: UnstructurizedMessageStory,
    ) -> Result<Self, ValueError> {
        let mut transferred_history: BTreeMap<DateTime<Utc>, MessageContent> = BTreeMap::new();
        let seen_by: BTreeMap<Uuid, DateTime<Utc>> = BTreeMap::new();

        for (history, (attachment_models, file_metadata_models), reaction_models) in history {
            let attachments_transferred = attachment_models
                .into_iter()
                .zip(file_metadata_models.into_iter())
                .map(|(attachment_model, file_metadata_model)| {
                    Attachment::from(attachment_model, file_metadata_model)
                })
                .collect::<Result<Vec<Attachment>, _>>()?;

            let reactions_transferred: Vec<Reaction> = if reaction_models.is_empty() {
                Vec::new()
            } else {
                reaction_models
                    .into_iter()
                    .map(|(reaction_model, reaction_history)| {
                        Reaction::from((reaction_model, reaction_history))
                    })
                    .collect::<Result<Vec<Reaction>, _>>()?
            };

            let message_content = MessageContent {
                id: history.id,
                text: history.text,
                attachments: if attachments_transferred.is_empty() {
                    Vec::new()
                } else {
                    attachments_transferred
                },
                reactions: reactions_transferred,
            };

            transferred_history.insert(history.created_at, message_content);
        }

        Ok(Self {
            id: message.id,
            author_id: message.author_id,
            chat_id: message.chat_id,
            updated_at: message.updated_at,
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
        reactions: Vec<Reaction>,
        reply_to_id: Option<Uuid>,
        available_for: Option<TimeStep>,
        attachments: Vec<Attachment>,
    ) -> Self {
        let now = Utc::now();
        let mut history = BTreeMap::new();

        let message_content = MessageContent {
            id: Uuid::new_v4(),
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
            updated_at: now,
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
            let mut updated_reactions = latest_content.reactions.clone();

            updated_reactions.push(reaction);

            let new_content = MessageContent {
                id: Uuid::new_v4(),
                text: latest_content.text.clone(),
                attachments: latest_content.attachments.clone(),
                reactions: updated_reactions,
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

        if let Some(index) = latest_content
            .reactions
            .iter()
            .position(|r| r.id == reaction_id)
        {
            let mut updated_reactions = latest_content.reactions.clone();
            updated_reactions.remove(index);

            let new_reactions = if updated_reactions.is_empty() {
                Vec::new()
            } else {
                updated_reactions
            };

            let new_content = MessageContent {
                id: Uuid::new_v4(),
                text: latest_content.text.clone(),
                attachments: latest_content.attachments.clone(),
                reactions: new_reactions,
            };

            self.history.insert(now, new_content);
            return Ok(self);
        }

        Err(MessageError::ReactionNotFound(reaction_id))
    }

    pub fn edit_content(&mut self, new_text: String) -> Result<&mut Self, MessageError> {
        let now = Utc::now();

        let latest_content = self
            .history
            .values()
            .last()
            .ok_or(MessageError::EmptyHistory)?;

        let new_content = MessageContent {
            id: Uuid::new_v4(),
            text: new_text,
            attachments: latest_content.attachments.clone(),
            reactions: latest_content.reactions.clone(),
        };

        self.history.insert(now, new_content);

        Ok(self)
    }
}
