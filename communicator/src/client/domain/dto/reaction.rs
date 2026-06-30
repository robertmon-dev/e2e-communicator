use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reaction {
    pub id: Uuid,

    pub author_id: Uuid,

    pub created_at: DateTime<Utc>,
    pub history: BTreeMap<DateTime<Utc>, String>,
}

impl Reaction {
    pub fn new(author_id: Uuid, initial_reaction: String) -> Self {
        let now = Utc::now();
        let mut history = BTreeMap::new();

        history.insert(now, initial_reaction);

        Self {
            id: Uuid::new_v4(),
            author_id,
            created_at: now,
            history,
        }
    }

    pub fn update_reaction(&mut self, new_reaction: String) {
        self.history.insert(Utc::now(), new_reaction);
    }

    pub fn current_reaction(&self) -> Option<&String> {
        self.history.values().last()
    }

    pub fn get_latest_with_date(&self) -> Option<(&DateTime<Utc>, &String)> {
        self.history.iter().last()
    }
}
