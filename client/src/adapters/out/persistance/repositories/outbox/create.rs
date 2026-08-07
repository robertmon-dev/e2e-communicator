use sea_orm::{DbErr, IntoActiveModel, TransactionTrait};

use crate::adapters::out::persistance::entities::outbox;
use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::utils::safe_bulk_insert;
use crate::domain::dto::outbox::OutboxEntry;

pub trait Create {
    async fn create_many(&self, entries: Vec<OutboxEntry>) -> Result<(), DbErr>;
}

impl Create for OutboxRepository {
    async fn create_many(&self, entries: Vec<OutboxEntry>) -> Result<(), DbErr> {
        let entries_active: Vec<outbox::ActiveModel> = entries
            .iter()
            .map(|entry| entry.into_model().into_active_model())
            .collect();

        let txn = self.db.begin().await?;
        safe_bulk_insert::<outbox::Entity, _, _>(&txn, entries_active).await?;
        txn.commit().await?;

        Ok(())
    }
}
