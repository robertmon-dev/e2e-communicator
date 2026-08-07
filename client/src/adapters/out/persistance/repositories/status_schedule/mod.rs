pub mod create;
pub mod update;

use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::adapters::out::persistance::entities::status_schedule;
use crate::domain::dto::status_schedule::{ScheduleSyncStatus, StatusSchedule};

pub struct StatusScheduleRepository {
    db: Arc<DatabaseConnection>,
}

impl StatusScheduleRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn get(&self, id: Uuid) -> Result<StatusSchedule, DbErr> {
        let persisted_entry = status_schedule::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound(
                    "Status schedule entry with such ID has not been found".to_string(),
                )
            })?;

        StatusSchedule::from(persisted_entry)
            .map_err(|e| DbErr::Custom(format!("Failed to structurize status schedule entry: {e}")))
    }

    pub async fn list_unsynced(&self, limit: u64) -> Result<Vec<StatusSchedule>, DbErr> {
        let persisted_entries = status_schedule::Entity::find()
            .filter(status_schedule::Column::SyncStatus.eq(i32::from(ScheduleSyncStatus::Pending)))
            .order_by_asc(status_schedule::Column::CreatedAt)
            .limit(limit)
            .all(self.db.as_ref())
            .await?;

        persisted_entries
            .into_iter()
            .map(StatusSchedule::from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DbErr::Custom(format!("Failed to structurize status schedule entry: {e}")))
    }
}
