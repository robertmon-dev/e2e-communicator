use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::status_schedule;
use crate::client::domain::dto::errors::ValueError;

#[derive(Debug, Copy, Clone, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(i32)]
pub enum PresenceStatus {
    Online = 0,
    Offline = 1,
}

impl TryFrom<i32> for PresenceStatus {
    type Error = ValueError;

    fn try_from(i: i32) -> Result<Self, ValueError> {
        match i {
            0 => Ok(Self::Online),
            1 => Ok(Self::Offline),
            n if n < 0 => Err(ValueError::LesserThanExpected),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

impl From<PresenceStatus> for i32 {
    fn from(status: PresenceStatus) -> Self {
        status as i32
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(i32)]
pub enum ScheduleSyncStatus {
    Pending = 0,
    Synced = 1,
}

impl TryFrom<i32> for ScheduleSyncStatus {
    type Error = ValueError;

    fn try_from(i: i32) -> Result<Self, ValueError> {
        match i {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Synced),
            n if n < 0 => Err(ValueError::LesserThanExpected),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

impl From<ScheduleSyncStatus> for i32 {
    fn from(status: ScheduleSyncStatus) -> Self {
        status as i32
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StatusSchedule {
    pub id: Uuid,

    pub target_status: PresenceStatus,
    pub scheduled_for: DateTime<Utc>,
    pub sync_status: ScheduleSyncStatus,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StatusSchedule {
    pub fn new(
        id: Uuid,
        target_status: PresenceStatus,
        scheduled_for: DateTime<Utc>,
        sync_status: ScheduleSyncStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            target_status,
            scheduled_for,
            sync_status,
            created_at,
            updated_at,
        }
    }

    pub fn pending(target_status: PresenceStatus, scheduled_for: DateTime<Utc>) -> Self {
        let now = Utc::now();

        Self {
            id: Uuid::new_v4(),
            target_status,
            scheduled_for,
            sync_status: ScheduleSyncStatus::Pending,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn from(model: status_schedule::Model) -> Result<Self, ValueError> {
        let target_status = PresenceStatus::try_from(model.target_status)?;
        let sync_status = ScheduleSyncStatus::try_from(model.sync_status)?;

        Ok(Self {
            id: model.id,
            target_status,
            scheduled_for: model.scheduled_for,
            sync_status,
            created_at: model.created_at,
            updated_at: model.updated_at,
        })
    }

    pub fn into_model(&self) -> status_schedule::Model {
        status_schedule::Model {
            id: self.id,
            target_status: self.target_status.into(),
            scheduled_for: self.scheduled_for,
            sync_status: self.sync_status.into(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
