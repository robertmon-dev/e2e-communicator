use chrono::Utc;
use sea_orm::{DatabaseConnection, DbErr, EntityTrait, IntoActiveModel, TransactionTrait};
use std::sync::Arc;
use tonic::async_trait;
use uuid::Uuid;

use crate::adapters::out::keys::DeviceKeyStoreError;
use crate::adapters::out::persistance::entities::local_identity;
use crate::adapters::out::persistance::repositories::utils::safe_bulk_insert;
use crate::domain::dto::local_identity::LocalIdentity;

#[async_trait]
pub trait DeviceKeyStore: Send + Sync {
    async fn get_or_create_keypair(&self, device_id: Uuid) -> Result<Vec<u8>, DeviceKeyStoreError>;
}

pub struct LocalIdentityRepository {
    db: Arc<DatabaseConnection>,
    device_key_store: Arc<dyn DeviceKeyStore>,
}

impl LocalIdentityRepository {
    pub fn new(db: Arc<DatabaseConnection>, device_key_store: Arc<dyn DeviceKeyStore>) -> Self {
        Self {
            db,
            device_key_store,
        }
    }

    pub async fn get_or_create(&mut self) -> Result<LocalIdentity, DbErr> {
        if let Some(existing) = local_identity::Entity::find().one(self.db.as_ref()).await? {
            return Ok(LocalIdentity::from(existing));
        }

        let device_id = Uuid::new_v4();
        let public_key = self
            .device_key_store
            .get_or_create_keypair(device_id)
            .await
            .map_err(|e| DbErr::Custom(e.to_string()))?;

        let identity = LocalIdentity {
            user_id: Uuid::new_v4(),
            device_id,
            public_key,
            created_at: Utc::now(),
        };

        let active_model = identity.into_model().into_active_model();
        let txn = self.db.begin().await?;
        safe_bulk_insert::<local_identity::Entity, _, _>(&txn, vec![active_model]).await?;
        txn.commit().await?;
        Ok(identity)
    }
}
