use chrono::Utc;
use sea_orm::{ActiveModelTrait, DbErr, EntityTrait, IntoActiveModel, Set};
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::outbox;
use crate::client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::client::domain::dto::outbox::{OutboxEntry, OutboxStatus};

pub trait Update {
    async fn mark_sent(&self, id: Uuid) -> Result<OutboxEntry, DbErr>;
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
}
