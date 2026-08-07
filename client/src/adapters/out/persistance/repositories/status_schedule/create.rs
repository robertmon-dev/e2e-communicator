use sea_orm::{ActiveModelTrait, DbErr, IntoActiveModel};

use crate::client::adapters::out::persistance::repositories::status_schedule::StatusScheduleRepository;
use crate::client::domain::dto::status_schedule::StatusSchedule;

pub trait Create {
    async fn create(&self, entry: StatusSchedule) -> Result<(), DbErr>;
}

impl Create for StatusScheduleRepository {
    async fn create(&self, entry: StatusSchedule) -> Result<(), DbErr> {
        let entry_active = entry.into_model().into_active_model();
        entry_active.insert(self.db.as_ref()).await?;

        Ok(())
    }
}
