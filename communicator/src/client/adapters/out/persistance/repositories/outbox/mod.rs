pub mod create;
pub mod update;

use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::outbox;
use crate::client::domain::dto::outbox::{OutboxEntry, OutboxStatus};

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

    pub async fn list_pending(&self, limit: u64) -> Result<Vec<OutboxEntry>, DbErr> {
        let persisted_entries = outbox::Entity::find()
            .filter(outbox::Column::Status.eq(i32::from(OutboxStatus::Pending)))
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
