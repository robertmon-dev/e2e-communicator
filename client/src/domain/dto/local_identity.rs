use chrono::{DateTime, Utc};
use common::identity::DevicePublicKeys;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::adapters::out::persistance::entities::local_identity;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct LocalIdentity {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub agreement_public_key: Vec<u8>,
    pub signing_public_key: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

impl LocalIdentity {
    pub fn new(user_id: Uuid, device_id: Uuid, keys: &DevicePublicKeys) -> Self {
        Self {
            user_id,
            device_id,
            agreement_public_key: keys.agreement_key.to_vec(),
            signing_public_key: keys.signing_key.to_vec(),
            created_at: Utc::now(),
        }
    }

    pub fn from(model: local_identity::Model) -> Self {
        Self {
            user_id: model.user_id,
            device_id: model.device_id,
            agreement_public_key: model.public_key,
            signing_public_key: model.signing_public_key,
            created_at: model.created_at,
        }
    }

    pub fn into_model(&self) -> local_identity::Model {
        local_identity::Model {
            device_id: self.device_id,
            user_id: self.user_id,
            public_key: self.agreement_public_key.clone(),
            signing_public_key: self.signing_public_key.clone(),
            created_at: self.created_at,
        }
    }
}
