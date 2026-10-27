use chrono::{DateTime, Utc};
use common::identity::FINGERPRINT_LEN;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use uuid::Uuid;

use crate::adapters::out::persistance::entities::invitation;
use crate::domain::dto::errors::ValueError;

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InvitationDirection {
    Outgoing = 0,
    Incoming = 1,
}

impl TryFrom<i32> for InvitationDirection {
    type Error = ValueError;

    fn try_from(i: i32) -> Result<Self, ValueError> {
        match i {
            0 => Ok(Self::Outgoing),
            1 => Ok(Self::Incoming),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

impl From<InvitationDirection> for i32 {
    fn from(direction: InvitationDirection) -> Self {
        direction as i32
    }
}

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InvitationStatus {
    Pending = 0,
    Accepted = 1,
    Rejected = 2,
    Cancelled = 3,
}

impl TryFrom<i32> for InvitationStatus {
    type Error = ValueError;

    fn try_from(i: i32) -> Result<Self, ValueError> {
        match i {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Accepted),
            2 => Ok(Self::Rejected),
            3 => Ok(Self::Cancelled),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

impl From<InvitationStatus> for i32 {
    fn from(status: InvitationStatus) -> Self {
        status as i32
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Invitation {
    pub id: Uuid,
    pub chat_id: Uuid,
    pub inviter_user_id: Uuid,
    pub invitee_user_id: Uuid,
    pub direction: InvitationDirection,
    pub status: InvitationStatus,
    pub expected_fingerprint: Option<[u8; FINGERPRINT_LEN]>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub responded_at: Option<DateTime<Utc>>,
}

impl Invitation {
    pub fn outgoing(
        chat_id: Uuid,
        inviter_user_id: Uuid,
        invitee_user_id: Uuid,
        expected_fingerprint: Option<[u8; FINGERPRINT_LEN]>,
    ) -> Self {
        let now = Utc::now();

        Self {
            id: Uuid::new_v4(),
            chat_id,
            inviter_user_id,
            invitee_user_id,
            direction: InvitationDirection::Outgoing,
            status: InvitationStatus::Pending,
            expected_fingerprint,
            created_at: now,
            updated_at: now,
            responded_at: None,
        }
    }

    pub fn incoming(id: Uuid, chat_id: Uuid, inviter_user_id: Uuid, invitee_user_id: Uuid) -> Self {
        let now = Utc::now();

        Self {
            id,
            chat_id,
            inviter_user_id,
            invitee_user_id,
            direction: InvitationDirection::Incoming,
            status: InvitationStatus::Pending,
            expected_fingerprint: None,
            created_at: now,
            updated_at: now,
            responded_at: None,
        }
    }

    pub fn from(model: invitation::Model) -> Result<Self, ValueError> {
        let direction = InvitationDirection::try_from(model.direction)?;
        let status = InvitationStatus::try_from(model.status)?;

        let expected_fingerprint = match model.expected_fingerprint {
            Some(bytes) => {
                let fixed: [u8; FINGERPRINT_LEN] =
                    bytes.try_into().map_err(|_| ValueError::CorruptedData)?;
                Some(fixed)
            }
            None => None,
        };

        Ok(Self {
            id: model.id,
            chat_id: model.chat_id,
            inviter_user_id: model.inviter_user_id,
            invitee_user_id: model.invitee_user_id,
            direction,
            status,
            expected_fingerprint,
            created_at: model.created_at,
            updated_at: model.updated_at,
            responded_at: model.responded_at,
        })
    }

    pub fn into_model(&self) -> invitation::Model {
        invitation::Model {
            id: self.id,
            chat_id: self.chat_id,
            inviter_user_id: self.inviter_user_id,
            invitee_user_id: self.invitee_user_id,
            direction: self.direction.into(),
            status: self.status.into(),
            expected_fingerprint: self.expected_fingerprint.map(|f| f.to_vec()),
            created_at: self.created_at,
            updated_at: self.updated_at,
            responded_at: self.responded_at,
        }
    }
}
