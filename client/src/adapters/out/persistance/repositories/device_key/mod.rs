use chrono::Utc;
use sea_orm::{DatabaseConnection, DbErr, EntityTrait, IntoActiveModel, TransactionTrait};
use std::sync::Arc;
use uuid::Uuid;

use crate::adapters::out::persistance::entities::device_key;
use crate::adapters::out::persistance::repositories::utils::safe_bulk_insert;

pub struct DeviceKeyRepository {
    db: Arc<DatabaseConnection>,
}

impl DeviceKeyRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn get(&self, device_id: Uuid) -> Result<Option<device_key::Model>, DbErr> {
        device_key::Entity::find_by_id(device_id)
            .one(self.db.as_ref())
            .await
    }

    pub async fn create(
        &self,
        device_id: Uuid,
        public_key: Vec<u8>,
        encrypted_private_key: Vec<u8>,
        signing_public_key: Vec<u8>,
        encrypted_signing_private_key: Vec<u8>,
    ) -> Result<device_key::Model, DbErr> {
        let model = device_key::Model {
            device_id,
            public_key,
            encrypted_private_key,
            signing_public_key,
            encrypted_signing_private_key,
            created_at: Utc::now(),
        };

        let txn = self.db.begin().await?;
        safe_bulk_insert::<device_key::Entity, _, _>(&txn, vec![model.clone().into_active_model()])
            .await?;
        txn.commit().await?;

        Ok(model)
    }
}
