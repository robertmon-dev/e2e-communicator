use chrono::Utc;
use sea_orm::{
    DatabaseConnection, DbErr, EntityTrait, IntoActiveModel, QueryOrder, TransactionTrait,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::adapters::out::persistance::entities::user_identity;
use crate::adapters::out::persistance::repositories::utils::safe_bulk_insert;
use crate::domain::dto::errors::IdentityStoreError;

pub struct UserIdentityRepository {
    db: Arc<DatabaseConnection>,
}

impl UserIdentityRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn get(&self) -> Result<Option<user_identity::Model>, DbErr> {
        user_identity::Entity::find()
            .order_by_asc(user_identity::Column::CreatedAt)
            .one(self.db.as_ref())
            .await
    }

    pub async fn create(
        &self,
        user_id: Uuid,
        signing_public_key: Vec<u8>,
        encrypted_signing_private_key: Vec<u8>,
    ) -> Result<user_identity::Model, IdentityStoreError> {
        if self.get().await?.is_some() {
            return Err(IdentityStoreError::AlreadyExists);
        }

        let model = user_identity::Model {
            user_id,
            signing_public_key,
            encrypted_signing_private_key,
            created_at: Utc::now(),
        };

        let txn = self.db.begin().await?;

        if user_identity::Entity::find().one(&txn).await?.is_some() {
            return Err(IdentityStoreError::AlreadyExists);
        }

        safe_bulk_insert::<user_identity::Entity, _, _>(
            &txn,
            vec![model.clone().into_active_model()],
        )
        .await?;
        txn.commit().await?;

        Ok(model)
    }

    pub async fn remove(&self) -> Result<(), DbErr> {
        let txn = self.db.begin().await?;
        user_identity::Entity::delete_many().exec(&txn).await?;
        txn.commit().await
    }
}
