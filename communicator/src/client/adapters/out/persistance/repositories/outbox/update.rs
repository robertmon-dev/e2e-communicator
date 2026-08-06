use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, DbErr, EntityTrait, IntoActiveModel, Set};
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::outbox;
use crate::client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::client::domain::dto::outbox::{OutboxEntry, OutboxStatus};

pub trait Update {
    async fn mark_sent(&self, id: Uuid) -> Result<OutboxEntry, DbErr>;
    async fn mark_failed(
        &self,
        id: Uuid,
        next_retry_at: DateTime<Utc>,
    ) -> Result<OutboxEntry, DbErr>;
}

impl Update for OutboxRepository {
    async fn mark_sent(&self, id: Uuid) -> Result<OutboxEntry, DbErr> {
        let persisted_entry = outbox::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Outbox entry with such ID has not been found".to_string())
            })?;

        let mut entry_active = persisted_entry.into_active_model();
        entry_active.status = Set(i32::from(OutboxStatus::Sent));
        entry_active.updated_at = Set(Utc::now());

        let updated_model = entry_active.update(self.db.as_ref()).await?;

        OutboxEntry::from(updated_model)
            .map_err(|e| DbErr::Custom(format!("Failed to structurize outbox entry: {e}")))
    }

    async fn mark_failed(
        &self,
        id: Uuid,
        next_retry_at: DateTime<Utc>,
    ) -> Result<OutboxEntry, DbErr> {
        let persisted_entry = outbox::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Outbox entry with such ID has not been found".to_string())
            })?;

        let attempts = persisted_entry.attempts;
        let mut entry_active = persisted_entry.into_active_model();
        entry_active.status = Set(i32::from(OutboxStatus::Failed));
        entry_active.attempts = Set(attempts + 1);
        entry_active.next_retry_at = Set(Some(next_retry_at));
        entry_active.updated_at = Set(Utc::now());

        let updated_model = entry_active.update(self.db.as_ref()).await?;

        OutboxEntry::from(updated_model)
            .map_err(|e| DbErr::Custom(format!("Failed to structurize outbox entry: {e}")))
    }
}
