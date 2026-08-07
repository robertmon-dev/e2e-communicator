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

impl TryFrom<u8> for ChatType {
    type Error = ValueError;

    fn try_from(i: u8) -> Result<Self, ValueError> {
        match i {
            0 => Ok(Self::Direct),
            1 => Ok(Self::Group),
            2 => Ok(Self::Channel),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

impl From<ChatType> for u8 {
    fn from(chat_type: ChatType) -> Self {
        chat_type as u8
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
pub type UnstructurizedChat = (chat::Model, ChatHistory);

impl From<Chat> for UnstructurizedChat {
    fn from(chat: Chat) -> Self {
        let chat_model = chat::Model {
            id: chat.id,
            is_active: true,
            created_at: chat.created_at,
            updated_at: chat.history.keys().last().copied().unwrap_or_else(Utc::now),
        };

        let mut history_vec = Vec::new();

        for (timestamp, state) in chat.history {
            let history_id = Uuid::new_v4();
            let chat_type = u8::from(state.chat_type);

            let history_model = chat_history::Model {
                id: history_id,
                chat_id: chat.id,
                name: state.name.unwrap_or_default(),
                chat_type,
                chat_theme: state.chat_theme.map(|url| url.to_string()),
                created_at: timestamp,
            };

            let mut members_vec = Vec::new();

            for (user_id, role) in state.members {
                let member_model = chat_member::Model {
                    id: Uuid::new_v4(),
                    role: role.into(),
                    chat_history_id: history_id,
                    user_id,
                };
                members_vec.push(member_model);
            }

            history_vec.push((history_model, members_vec));
        }

        (chat_model, history_vec)
    }
}

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

    pub fn allign_messages(&mut self, messages: Vec<Message>) -> Result<&mut Self, ChatError> {
        for message in messages {
            if message.chat_id != self.id {
                return Err(ChatError::ChatNotFound(message.chat_id));
            } else {
                self.messages.insert(message.id, message);
            }
        }

        Ok(self)
    }

    pub fn from(chat: chat::Model, history: ChatHistory) -> Result<Self, ValueError> {
        let mut story_tree = BTreeMap::new();

        for (story, members) in history {
            let mut members_tree = BTreeMap::new();
            for member in members {
                let role = MemberRole::try_from(member.role)?;
                members_tree.insert(member.user_id, role);
            }

            let chat_theme: Option<Url> = if let Some(theme_link) = story.chat_theme {
                Some(Url::parse(theme_link.as_str())?)
            } else {
                None
            };

            let chat_type = ChatType::try_from(story.chat_type)?;
            let chat_state = ChatState {
                name: Some(story.name),
                chat_type,
                chat_theme,
                members: members_tree,
            };

            story_tree.insert(story.created_at, chat_state);
        }

        Ok(Self {
            id: chat.id,
            messages: BTreeMap::new(),
            created_at: chat.created_at,
            history: story_tree,
        })
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
