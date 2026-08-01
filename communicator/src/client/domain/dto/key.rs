use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::key;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Key {
    pub id: Uuid,

    pub public_key: Vec<u8>,
    pub encrypted_private_key: Vec<u8>,

    pub chat_id: Uuid,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Key {
    fn new(
        id: Uuid,
        chat_id: Uuid,
        public_key: Vec<u8>,
        encrypted_private_key: Vec<u8>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            chat_id,
            encrypted_private_key,
            public_key,
            created_at,
            updated_at,
        }
    }

    fn from(key_model: key::Model) -> Self {
        Self {
            id: key_model.id,
            chat_id: key_model.chat_id,
            public_key: key_model.public_key,
            encrypted_private_key: key_model.encrypted_private_key,
            created_at: key_model.created_at,
            updated_at: key_model.updated_at,
        }
    }
}
