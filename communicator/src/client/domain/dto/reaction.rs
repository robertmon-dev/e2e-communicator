use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::{reaction, reaction_history};
use crate::client::domain::dto::errors::{ReactionError, ValueError};

pub type UnstructurizedReaction = (reaction::Model, Vec<reaction_history::Model>);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReactionContent {
    pub id: Uuid,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reaction {
    pub id: Uuid,

    pub chat_id: Uuid,

    pub message_history_id: Uuid,
    pub author_id: Uuid,

    pub created_at: DateTime<Utc>,
    pub history: BTreeMap<DateTime<Utc>, ReactionContent>,
}

impl Reaction {
    pub fn new(
        chat_id: Uuid,
        author_id: Uuid,
        message_history_id: Uuid,
        initial_reaction: String,
    ) -> Self {
        let now = Utc::now();
        let mut history = BTreeMap::new();

        let reaction_content = ReactionContent {
            id: Uuid::new_v4(),
            text: initial_reaction,
        };

        history.insert(now, reaction_content);

        Self {
            id: Uuid::new_v4(),
            chat_id,
            message_history_id,
            author_id,
            created_at: now,
            history,
        }
    }

    pub fn from(unstructurized_reaction: UnstructurizedReaction) -> Result<Reaction, ValueError> {
        let (reaction_model, history) = unstructurized_reaction;
        let mut history_tree = BTreeMap::new();

        for history_record in history.iter() {
            let reaction_content = ReactionContent {
                id: history_record.id,
                text: history_record.text.clone(),
            };

            history_tree.insert(history_record.created_at, reaction_content);
        }

        let transformed_reaction = Self {
            id: reaction_model.id,
            chat_id: reaction_model.chat_id,
            message_history_id: reaction_model.message_history_id,
            author_id: reaction_model.author_id,
            created_at: reaction_model.created_at,
            history: history_tree,
        };

        Ok(transformed_reaction)
    }

    pub fn update_reaction(&mut self, new_reaction: String) {
        self.history.insert(
            Utc::now(),
            ReactionContent {
                id: Uuid::new_v4(),
                text: new_reaction,
            },
        );
    }

    pub fn current_reaction(&self) -> Option<&ReactionContent> {
        self.history.values().last()
    }

    pub fn get_latest_with_date(&self) -> Option<(&DateTime<Utc>, &ReactionContent)> {
        self.history.iter().last()
    }
}
