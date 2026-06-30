use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::BTreeMap;
use uuid::Uuid;

use super::permission::MemberRole;

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum ChatType {
    Direct = 0,
    Group = 1,
    Channel = 2,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatState {
    pub name: Option<String>,
    pub members: BTreeMap<Uuid, MemberRole>,
    pub chat_type: ChatType,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Chat {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub history: BTreeMap<DateTime<Utc>, ChatState>,
}

impl Chat {
    pub fn new(
        chat_type: ChatType,
        name: Option<String>,
        initial_members: BTreeMap<Uuid, MemberRole>,
    ) -> Self {
        let now = Utc::now();
        let initial_state = ChatState {
            name,
            members: initial_members,
            chat_type,
        };

        let mut history = BTreeMap::new();
        history.insert(now, initial_state);

        Self {
            id: Uuid::new_v4(),
            created_at: now,
            history,
        }
    }

    pub fn get_latest(&self) -> &ChatState {
        self.history
            .values()
            .last()
            .expect("Chat history should never be empty")
    }

    pub fn update_name(&mut self, new_name: Option<String>) {
        let mut new_state = self.get_latest().clone();
        new_state.name = new_name;
        self.history.insert(Utc::now(), new_state);
    }

    pub fn add_member(&mut self, member_id: Uuid, role: MemberRole) {
        let mut new_state = self.get_latest().clone();
        new_state.members.insert(member_id, role);
        self.history.insert(Utc::now(), new_state);
    }

    pub fn remove_member(&mut self, member_id: &Uuid) {
        let mut new_state = self.get_latest().clone();
        if new_state.members.remove(member_id).is_some() {
            self.history.insert(Utc::now(), new_state);
        }
    }
}
