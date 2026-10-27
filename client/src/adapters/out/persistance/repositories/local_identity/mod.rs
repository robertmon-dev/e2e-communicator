use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    DatabaseConnection, DbErr, EntityTrait, IntoActiveModel, QueryOrder, Set, TransactionTrait,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::adapters::out::keys::{DeviceKeyStore, DeviceKeyStoreRepository};
use crate::adapters::out::persistance::entities::{device_key, local_identity, user};
use crate::adapters::out::persistance::repositories::utils::safe_bulk_insert;
use crate::domain::dto::errors::IdentityStoreError;
use crate::domain::dto::local_identity::LocalIdentity;

const DEFAULT_NICKNAME_PREFIX: &str = "user-";

pub struct LocalIdentityRepository {
    db: Arc<DatabaseConnection>,
    device_key_store: Arc<DeviceKeyStore>,
}

impl LocalIdentityRepository {
    pub fn new(db: Arc<DatabaseConnection>, device_key_store: Arc<DeviceKeyStore>) -> Self {
        Self {
            db,
            device_key_store,
        }
    }

    pub async fn get(&self) -> Result<Option<LocalIdentity>, DbErr> {
        Ok(local_identity::Entity::find()
            .order_by_asc(local_identity::Column::CreatedAt)
            .one(self.db.as_ref())
            .await?
            .map(LocalIdentity::from))
    }

    pub async fn create(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        device_cert: Vec<u8>,
    ) -> Result<LocalIdentity, IdentityStoreError> {
        if self.get().await?.is_some() {
            return Err(IdentityStoreError::AlreadyExists);
        }

        let keys = self.device_key_store.get_or_create_keys(device_id).await?;

        let identity = LocalIdentity::new(user_id, device_id, &keys, device_cert);
        let now = Utc::now();
        let device_id_hex = device_id.simple().to_string();
        let short_device = &device_id_hex[..8];

        let txn = self.db.begin().await?;

        if local_identity::Entity::find().one(&txn).await?.is_some() {
            return Err(IdentityStoreError::AlreadyExists);
        }

        safe_bulk_insert::<local_identity::Entity, _, _>(
            &txn,
            vec![identity.into_model().into_active_model()],
        )
        .await?;

        user::Entity::insert(user::ActiveModel {
            id: Set(user_id),
            name: Set(None),
            surname: Set(None),
            nickname: Set(format!("{DEFAULT_NICKNAME_PREFIX}{short_device}")),
            phone: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            signing_public_key: Set(None),
            verified_at: Set(None),
        })
        .on_conflict(OnConflict::column(user::Column::Id).do_nothing().to_owned())
        .exec_without_returning(&txn)
        .await?;

        txn.commit().await?;
        Ok(identity)
    }

    pub async fn remove(&self) -> Result<(), DbErr> {
        let txn = self.db.begin().await?;
        local_identity::Entity::delete_many().exec(&txn).await?;
        device_key::Entity::delete_many().exec(&txn).await?;
        txn.commit().await
    }
}
