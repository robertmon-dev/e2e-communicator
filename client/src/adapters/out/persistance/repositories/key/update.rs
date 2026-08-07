use chrono::Utc;
use sea_orm::{ActiveModelTrait, DbErr, EntityTrait, IntoActiveModel, Set};

use crate::adapters::out::persistance::entities::key;
use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::adapters::out::persistance::repositories::key::utils::KeyUpdateInput;
use crate::domain::dto::key::Key;

pub trait Update {
    async fn update_key(&self, update_input: KeyUpdateInput) -> Result<Key, DbErr>;
}

impl Update for KeyRepository {
    async fn update_key(&self, update_input: KeyUpdateInput) -> Result<Key, DbErr> {
        let persisted_key = key::Entity::find_by_id(update_input.id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("No key persisted with such ID".to_string()))?;

        let mut key_active = persisted_key.into_active_model();

        if let Some(public_key) = update_input.public_key {
            key_active.public_key = Set(public_key);
        }

        if let Some(encrypted_private_key) = update_input.encrypted_private_key {
            key_active.encrypted_private_key = Set(Some(encrypted_private_key));
        }

        key_active.updated_at = Set(Utc::now());

        let updated_model = key_active.update(self.db.as_ref()).await?;

        Ok(Key::from(updated_model))
    }
}
