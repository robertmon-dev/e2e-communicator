use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::client::domain::dto::{errors::MessageError, reaction::Reaction};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageContent {
    pub text: String,
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
    pub hidden: bool,

    pub is_reply: bool,
    pub reply_to_id: Option<Uuid>,
}

impl Message {
    pub fn new(
        author_id: Uuid,
        chat_id: Uuid,
        initial_message: String,
        reactions: Vec<Reaction>,
        is_reply: bool,
        reply_to_id: Option<Uuid>,
    ) -> Self {
        let now = Utc::now();
        let mut history = BTreeMap::new();

        let message_content = MessageContent {
            text: initial_message,
            reactions,
        };

        history.insert(now, message_content);

        Self {
            id: Uuid::new_v4(),
            chat_id,
            author_id,
            created_at: now,

            history,
            seen_by: BTreeMap::new(),

            hidden: false,

            is_reply,
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
                text: latest_content.text.clone(),
                reactions: updated_reactions,
            };

            self.history.insert(now, new_content);
        }

        self
    }

    pub fn remove_reaction(&mut self, reaction_id: Uuid) -> Result<&mut Self, MessageError> {
        let now = Utc::now();

        let latest = self
            .history
            .values()
            .last()
            .ok_or(MessageError::EmptyHistory)?;

        if let Some(index) = latest.reactions.iter().position(|r| r.id == reaction_id) {
            let mut updated_reactions = latest.reactions.clone();
            updated_reactions.remove(index);

            let new_content = MessageContent {
                text: latest.text.clone(),
                reactions: updated_reactions,
            };

            self.history.insert(now, new_content);
            Ok(self)
        } else {
            Err(MessageError::ReactionNotFound(reaction_id))
        }
    }
}
