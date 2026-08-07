use sea_orm::{DbErr, IntoActiveModel, TransactionTrait};

use crate::client::adapters::out::persistance::entities::key;
use crate::client::adapters::out::persistance::repositories::key::KeyRepository;
use crate::client::adapters::out::persistance::repositories::utils::{
    safe_bulk_insert, safe_bulk_upsert,
};
use crate::client::domain::dto::key::Key;

pub trait Create {
    async fn create(&self, key: Key) -> Result<(), DbErr>;
    async fn create_many(&self, keys: Vec<Key>) -> Result<(), DbErr>;
    async fn upsert(&self, key: Key) -> Result<(), DbErr>;
    async fn upsert_many(&self, keys: Vec<Key>) -> Result<(), DbErr>;
}

impl Create for KeyRepository {
    async fn create(&self, key: Key) -> Result<(), DbErr> {
        let key_model_active = key.into_model().into_active_model();

        let txn = self.db.begin().await?;
        safe_bulk_insert::<key::Entity, _, _>(&txn, vec![key_model_active]).await?;
        txn.commit().await?;

        Ok(())
    }

    async fn create_many(&self, keys: Vec<Key>) -> Result<(), DbErr> {
        let keys_model_active: Vec<key::ActiveModel> = keys
            .iter()
            .map(|key| key.into_model().into_active_model())
            .collect();

        let txn = self.db.begin().await?;
        safe_bulk_insert::<key::Entity, _, _>(&txn, keys_model_active).await?;
        txn.commit().await?;

        Ok(())
    }

    async fn upsert(&self, key: Key) -> Result<(), DbErr> {
        let key_model_active = key.into_model().into_active_model();

        let txn = self.db.begin().await?;
        safe_bulk_upsert::<key::Entity, _, _>(&txn, vec![key_model_active]).await?;
        txn.commit().await?;

        Ok(())
    }

    async fn upsert_many(&self, keys: Vec<Key>) -> Result<(), DbErr> {
        let keys_model_active: Vec<key::ActiveModel> = keys
            .iter()
            .map(|key| key.into_model().into_active_model())
            .collect();

        let txn = self.db.begin().await?;
        safe_bulk_upsert::<key::Entity, _, _>(&txn, keys_model_active).await?;
        txn.commit().await?;

        Ok(())
    }
}
