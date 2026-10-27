use common::identity::{KEY_LEN, SIGNATURE_LEN, canonical_chat_key_bytes, verify};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::dto::errors::InviteError;
use crate::domain::dto::identity::{cert_b64, key_b64};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ChatKeyBundle {
    pub chat_id: Uuid,
    pub user_id: Uuid,
    pub device_id: Uuid,
    #[serde(with = "key_b64")]
    pub chat_public_key: [u8; KEY_LEN],
    #[serde(with = "cert_b64")]
    pub signature: [u8; SIGNATURE_LEN],
}

impl ChatKeyBundle {
    pub fn signed<F>(
        chat_id: Uuid,
        user_id: Uuid,
        device_id: Uuid,
        chat_public_key: [u8; KEY_LEN],
        sign_with: F,
    ) -> Self
    where
        F: FnOnce(&[u8]) -> [u8; SIGNATURE_LEN],
    {
        let canonical = canonical_chat_key_bytes(chat_id, user_id, device_id, &chat_public_key);
        let signature = sign_with(&canonical);

        Self {
            chat_id,
            user_id,
            device_id,
            chat_public_key,
            signature,
        }
    }

    pub fn verify(&self, device_signing_key: &[u8; KEY_LEN]) -> Result<(), InviteError> {
        let canonical = canonical_chat_key_bytes(
            self.chat_id,
            self.user_id,
            self.device_id,
            &self.chat_public_key,
        );

        verify(device_signing_key, &canonical, &self.signature)
            .map_err(|_| InviteError::BrokenTrustChain(self.user_id, self.device_id))
    }
}
