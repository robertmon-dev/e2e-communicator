use sea_orm::{ActiveModelTrait, DbErr, IntoActiveModel, TransactionTrait};

use crate::adapters::out::persistance::entities::{
    file_metadata, profile_photo_history, user, user_overlay_history,
};
use crate::adapters::out::persistance::repositories::user::UserRepository;
use crate::adapters::out::persistance::repositories::utils::{safe_bulk_insert, safe_bulk_upsert};
use crate::domain::dto::user::{UnstructurizedUser, User};

pub trait Create {
    async fn create(&self, user: User) -> Result<(), DbErr>;
    async fn upsert(&self, user: User) -> Result<(), DbErr>;
}

impl Create for UserRepository {
    async fn create(&self, user: User) -> Result<(), DbErr> {
        let (user_model, photo_history, overlay_history) = UnstructurizedUser::from(user);

        let txn = self.db.begin().await?;

        let user_model_active = user_model.into_active_model();
        user_model_active.insert(&txn).await?;

        let mut file_metadata_active = Vec::new();
        let mut photo_history_active = Vec::new();

        for (photo_record, file_metadata_record) in photo_history.into_iter() {
            file_metadata_active.push(file_metadata_record.into_active_model());
            photo_history_active.push(photo_record.into_active_model());
        }

        if !file_metadata_active.is_empty() {
            safe_bulk_insert::<file_metadata::Entity, _, _>(&txn, file_metadata_active).await?;
        }

        if !photo_history_active.is_empty() {
            safe_bulk_insert::<profile_photo_history::Entity, _, _>(&txn, photo_history_active)
                .await?;
        }

        let overlay_history_active: Vec<_> = overlay_history
            .into_iter()
            .map(|record| record.into_active_model())
            .collect();

        if !overlay_history_active.is_empty() {
            safe_bulk_insert::<user_overlay_history::Entity, _, _>(&txn, overlay_history_active)
                .await?;
        }

        txn.commit().await?;

        Ok(())
    }

    async fn upsert(&self, user: User) -> Result<(), DbErr> {
        let (user_model, photo_history, overlay_history) = UnstructurizedUser::from(user);

        let txn = self.db.begin().await?;

        let user_model_active = user_model.into_active_model();
        safe_bulk_upsert::<user::Entity, _, _>(&txn, vec![user_model_active]).await?;

        let mut file_metadata_active = Vec::new();
        let mut photo_history_active = Vec::new();

        for (photo_record, file_metadata_record) in photo_history.into_iter() {
            file_metadata_active.push(file_metadata_record.into_active_model());
            photo_history_active.push(photo_record.into_active_model());
        }

        if !file_metadata_active.is_empty() {
            safe_bulk_upsert::<file_metadata::Entity, _, _>(&txn, file_metadata_active).await?;
        }

        if !photo_history_active.is_empty() {
            safe_bulk_upsert::<profile_photo_history::Entity, _, _>(&txn, photo_history_active)
                .await?;
        }

        let overlay_history_active: Vec<_> = overlay_history
            .into_iter()
            .map(|record| record.into_active_model())
            .collect();

        if !overlay_history_active.is_empty() {
            safe_bulk_upsert::<user_overlay_history::Entity, _, _>(&txn, overlay_history_active)
                .await?;
        }

        txn.commit().await?;

        Ok(())
    }
}
