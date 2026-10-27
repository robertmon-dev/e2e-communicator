use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, IntoActiveModel,
    QueryFilter, Set,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::adapters::out::persistance::entities::{peer_device, user};
use crate::adapters::out::persistance::repositories::utils::safe_bulk_upsert;

pub struct PeerDeviceRecord {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub signing_public_key: Vec<u8>,
    pub agreement_public_key: Vec<u8>,
    pub device_cert: Vec<u8>,
}

pub struct PeerDeviceRepository {
    db: Arc<DatabaseConnection>,
}

impl PeerDeviceRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn upsert_many(&self, records: Vec<PeerDeviceRecord>) -> Result<(), DbErr> {
        if records.is_empty() {
            return Ok(());
        }

        let pinned_at = Utc::now();
        let models: Vec<peer_device::ActiveModel> = records
            .into_iter()
            .map(|record| {
                peer_device::Model {
                    user_id: record.user_id,
                    device_id: record.device_id,
                    signing_public_key: record.signing_public_key,
                    agreement_public_key: record.agreement_public_key,
                    device_cert: record.device_cert,
                    pinned_at,
                }
                .into_active_model()
            })
            .collect();

        safe_bulk_upsert::<peer_device::Entity, _, _>(self.db.as_ref(), models).await
    }

    pub async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<peer_device::Model>, DbErr> {
        peer_device::Entity::find()
            .filter(peer_device::Column::UserId.eq(user_id))
            .all(self.db.as_ref())
            .await
    }

    pub async fn pin_identity(
        &self,
        user_id: Uuid,
        signing_public_key: Vec<u8>,
    ) -> Result<(), DbErr> {
        let persisted = user::Entity::find_by_id(user_id)
            .one(self.db.as_ref())
            .await?;

        let Some(persisted) = persisted else {
            let now = Utc::now();
            let short = &user_id.simple().to_string()[..8];

            user::ActiveModel {
                id: Set(user_id),
                name: Set(None),
                surname: Set(None),
                nickname: Set(format!("user-{short}")),
                phone: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                signing_public_key: Set(Some(signing_public_key)),
                verified_at: Set(None),
            }
            .insert(self.db.as_ref())
            .await?;

            return Ok(());
        };

        let changed =
            persisted.signing_public_key.as_deref() != Some(signing_public_key.as_slice());

        let mut active = persisted.into_active_model();
        active.signing_public_key = Set(Some(signing_public_key));
        if changed {
            active.verified_at = Set(None);
        }
        active.update(self.db.as_ref()).await?;

        Ok(())
    }

    pub async fn pinned_identity(&self, user_id: Uuid) -> Result<Option<Vec<u8>>, DbErr> {
        Ok(user::Entity::find_by_id(user_id)
            .one(self.db.as_ref())
            .await?
            .and_then(|model| model.signing_public_key))
    }

    pub async fn mark_verified(&self, user_id: Uuid) -> Result<(), DbErr> {
        let persisted = user::Entity::find_by_id(user_id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("No user persisted with such ID".to_string()))?;

        let mut active = persisted.into_active_model();
        active.verified_at = Set(Some(Utc::now()));
        active.update(self.db.as_ref()).await?;

        Ok(())
    }

    pub async fn verified_at(&self, user_id: Uuid) -> Result<Option<DateTime<Utc>>, DbErr> {
        Ok(user::Entity::find_by_id(user_id)
            .one(self.db.as_ref())
            .await?
            .and_then(|model| model.verified_at))
    }
}
