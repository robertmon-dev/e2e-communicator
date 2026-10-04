pub mod create;
pub mod update;

use chrono::Utc;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::adapters::out::persistance::entities::outbox;
use crate::domain::dto::outbox::{OutboxEntry, OutboxStatus};

pub struct OutboxRepository {
    db: Arc<DatabaseConnection>,
}

impl OutboxRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn get(&self, id: Uuid) -> Result<OutboxEntry, DbErr> {
        let persisted_entry = outbox::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Outbox entry with such ID has not been found".to_string())
            })?;

        OutboxEntry::from(persisted_entry)
            .map_err(|e| DbErr::Custom(format!("Failed to structurize outbox entry: {e}")))
    }

    pub async fn get_by_message_and_recipient(
        &self,
        message_id: Uuid,
        recipient_id: Uuid,
    ) -> Result<OutboxEntry, DbErr> {
        let persisted_entry = outbox::Entity::find()
            .filter(outbox::Column::MessageId.eq(message_id))
            .filter(outbox::Column::RecipientId.eq(recipient_id))
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound(
                    "Outbox entry for such message and recipient has not been found".to_string(),
                )
            })?;

        OutboxEntry::from(persisted_entry)
            .map_err(|e| DbErr::Custom(format!("Failed to structurize outbox entry: {e}")))
    }

    pub async fn list_due(&self, limit: u64) -> Result<Vec<OutboxEntry>, DbErr> {
        let now = Utc::now();
        let condition = Condition::any()
            .add(
                Condition::all()
                    .add(outbox::Column::Status.eq(i32::from(OutboxStatus::Pending)))
                    .add(
                        Condition::any()
                            .add(outbox::Column::ScheduledFor.is_null())
                            .add(outbox::Column::ScheduledFor.lte(now)),
                    ),
            )
            .add(
                Condition::all()
                    .add(outbox::Column::Status.eq(i32::from(OutboxStatus::Failed)))
                    .add(outbox::Column::NextRetryAt.lte(now)),
            );

        let persisted_entries = outbox::Entity::find()
            .filter(condition)
            .order_by_asc(outbox::Column::CreatedAt)
            .limit(limit)
            .all(self.db.as_ref())
            .await?;

        persisted_entries
            .into_iter()
            .map(OutboxEntry::from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DbErr::Custom(format!("Failed to structurize outbox entry: {e}")))
    }
}
