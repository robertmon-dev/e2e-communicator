use sea_orm::{DatabaseConnection, DbErr, EntityTrait, IntoActiveModel, TransactionTrait};
use std::sync::Arc;
use uuid::Uuid;

use crate::adapters::out::persistance::entities::local_identity;
use crate::adapters::out::persistance::repositories::utils::safe_bulk_insert;
use crate::domain::dto::local_identity::LocalIdentity;

pub struct LocalIdentityRepository {
    db: Arc<DatabaseConnection>,
}

impl LocalIdentityRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn get_or_create(&self) -> Result<LocalIdentity, DbErr> {
        if let Some(existing) = local_identity::Entity::find().one(self.db.as_ref()).await? {
            return Ok(LocalIdentity::from(existing));
        }

        let identity = LocalIdentity::new(Uuid::new_v4(), Uuid::new_v4());
        let active_model = identity.into_model().into_active_model();

        let txn = self.db.begin().await?;
        safe_bulk_insert::<local_identity::Entity, _, _>(&txn, vec![active_model]).await?;
        txn.commit().await?;

        Ok(identity)
    }
}
