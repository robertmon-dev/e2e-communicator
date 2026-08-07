use chrono::Utc;
use sea_orm::{ActiveModelTrait, DbErr, EntityTrait, IntoActiveModel, Set};
use uuid::Uuid;

use crate::adapters::out::persistance::entities::status_schedule;
use crate::adapters::out::persistance::repositories::status_schedule::StatusScheduleRepository;
use crate::domain::dto::status_schedule::{ScheduleSyncStatus, StatusSchedule};

pub trait Update {
    async fn mark_synced(&self, id: Uuid) -> Result<StatusSchedule, DbErr>;
}

impl Update for StatusScheduleRepository {
    async fn mark_synced(&self, id: Uuid) -> Result<StatusSchedule, DbErr> {
        let persisted_entry = status_schedule::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound(
                    "Status schedule entry with such ID has not been found".to_string(),
                )
            })?;

        let mut entry_active = persisted_entry.into_active_model();
        entry_active.sync_status = Set(i32::from(ScheduleSyncStatus::Synced));
        entry_active.updated_at = Set(Utc::now());

        let updated_model = entry_active.update(self.db.as_ref()).await?;

        StatusSchedule::from(updated_model)
            .map_err(|e| DbErr::Custom(format!("Failed to structurize status schedule entry: {e}")))
    }
}
