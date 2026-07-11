use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::BTreeMap;
use url::Url;
use uuid::Uuid;

use super::permission::MemberRole;
use crate::client::adapters::out::persistance::entities::{chat, chat_history, chat_member};
use crate::client::domain::dto::errors::ValueError;
use crate::client::domain::dto::{errors::ChatError, message::Message};

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum ChatType {
    Direct = 0,
    Group = 1,
    Channel = 2,
}

impl ChatType {
    pub fn from(n: u8) -> Result<Self, ValueError> {
        match n {
            0 => Ok(Self::Direct),
            1 => Ok(Self::Group),
            2 => Ok(Self::Channel),
            n if n < 0 => Err(ValueError::LesserThanExpected),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatState {
    pub name: Option<String>,
    pub members: BTreeMap<Uuid, MemberRole>,
    pub chat_type: ChatType,
    pub chat_theme: Option<Url>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Chat {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub history: BTreeMap<DateTime<Utc>, ChatState>,
    pub messages: BTreeMap<Uuid, Message>,
}

pub type ChatHistory = Vec<(chat_history::Model, Vec<chat_member::Model>)>;

impl Chat {
    pub fn new(
        chat_type: ChatType,
        name: Option<String>,
        initial_members: BTreeMap<Uuid, MemberRole>,
        messages: BTreeMap<Uuid, Message>,
        chat_theme: Option<Url>,
    ) -> Self {
        let now = Utc::now();
        let initial_state = ChatState {
            name,
            members: initial_members,
            chat_type,
            chat_theme,
        };

        let mut history = BTreeMap::new();
        history.insert(now, initial_state);

        Self {
            id: Uuid::new_v4(),
            created_at: now,
            history,
            messages,
        }
    }

    pub fn from(chat: chat::Model, history: ChatHistory) -> Result<Self, ValueError> {
        for (story, members) in history {
            let theme_url: Option<Url> = if let Some(theme_link) = story.chat_theme {
                Some(Url::parse(theme_link.as_str())?)
            } else {
                None
            };

            let chat_state = ChatState {
                name: Some(story.name),
                chat_type: story.chat_type,
                chat_theme: theme_url,
                members: BTreeMap::new(),
            };
        }

        Self {}
    }

    pub fn get_latest(&self) -> Result<&ChatState, ChatError> {
        match self.history.values().last() {
            Some(history) => Ok(history),
            None => Err(ChatError::ChatHistoryNotFound(self.id)),
        }
    }

    pub fn update_name(&mut self, new_name: Option<String>) -> Result<&mut Self, ChatError> {
        let current_state = self.get_latest()?;

        let mut new_state = current_state.clone();
        new_state.name = new_name;

        self.history.insert(Utc::now(), new_state);
        Ok(self)
    }

    pub fn add_member(
        &mut self,
        member_id: Uuid,
        role: MemberRole,
    ) -> Result<&mut Self, ChatError> {
        let current_state = self.get_latest()?;
        let mut new_state = current_state.clone();

        new_state.members.insert(member_id, role);
        self.history.insert(Utc::now(), new_state);

        Ok(self)
    }

    pub fn remove_member(&mut self, member_id: &Uuid) -> Result<&mut Self, ChatError> {
        let current_state = self.get_latest()?;
        let mut new_state = current_state.clone();

        if new_state.members.remove(member_id).is_some() {
            self.history.insert(Utc::now(), new_state);
        }

        Ok(self)
    }
}
