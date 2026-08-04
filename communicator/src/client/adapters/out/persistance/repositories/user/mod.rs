pub mod create;
pub mod remove;
pub mod update;
pub mod utils;

use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait, QueryFilter,
    Select,
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::utils::ConditionExt;
use crate::client::adapters::out::persistance::entities::{
    file_metadata, profile_photo_history, user, user_overlay_history,
};
use crate::client::domain::dto::user::User;
use crate::client::domain::dto::utils::PaginationMeta;

pub struct UserRepository {
    db: Arc<DatabaseConnection>,
}

impl UserRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    fn build_base_query(&self, restricted_user_ids: Option<Vec<Uuid>>) -> Select<user::Entity> {
        let conditions =
            Condition::all().add_opt(restricted_user_ids, |ids| user::Column::Id.is_in(ids));

        user::Entity::find().filter(conditions)
    }

    async fn assemble_users(&self, users: Vec<user::Model>) -> Result<Vec<User>, DbErr> {
        let user_ids: Vec<Uuid> = users.iter().map(|u| u.id).collect();

        let photo_histories = if user_ids.is_empty() {
            Vec::new()
        } else {
            profile_photo_history::Entity::find()
                .filter(profile_photo_history::Column::UserId.is_in(user_ids.clone()))
                .all(self.db.as_ref())
                .await?
        };

        let file_metadata_ids: Vec<Uuid> =
            photo_histories.iter().map(|p| p.file_metadata_id).collect();

        let file_metadatas = if file_metadata_ids.is_empty() {
            Vec::new()
        } else {
            file_metadata::Entity::find()
                .filter(file_metadata::Column::Id.is_in(file_metadata_ids))
                .all(self.db.as_ref())
                .await?
        };

        let metadata_by_id: HashMap<Uuid, file_metadata::Model> = file_metadatas
            .into_iter()
            .map(|meta| (meta.id, meta))
            .collect();

        let mut photo_histories_by_user_id: HashMap<
            Uuid,
            Vec<(profile_photo_history::Model, file_metadata::Model)>,
        > = HashMap::new();

        for photo in photo_histories {
            if let Some(meta) = metadata_by_id.get(&photo.file_metadata_id).cloned() {
                photo_histories_by_user_id
                    .entry(photo.user_id)
                    .or_default()
                    .push((photo, meta));
            }
        }

        let overlay_histories = if user_ids.is_empty() {
            Vec::new()
        } else {
            user_overlay_history::Entity::find()
                .filter(user_overlay_history::Column::UserId.is_in(user_ids))
                .all(self.db.as_ref())
                .await?
        };

        let mut overlay_histories_by_user_id: HashMap<Uuid, Vec<user_overlay_history::Model>> =
            HashMap::new();

        for overlay in overlay_histories {
            overlay_histories_by_user_id
                .entry(overlay.user_id)
                .or_default()
                .push(overlay);
        }

        let mut assembled_users = Vec::new();

        for user_model in users {
            let user_id = user_model.id;
            let photo_history = photo_histories_by_user_id
                .remove(&user_id)
                .unwrap_or_default();
            let overlay_history = overlay_histories_by_user_id
                .remove(&user_id)
                .unwrap_or_default();

            let transferred_user =
                User::from(user_model, photo_history, overlay_history).map_err(|e| {
                    DbErr::Custom(format!(
                        "Failed to structurize user DTO for user with ID: {}. Original error: {}",
                        user_id, e
                    ))
                })?;

            assembled_users.push(transferred_user);
        }

        Ok(assembled_users)
    }

    pub async fn list(
        &self,
        pagination_meta: PaginationMeta,
        restricted_user_ids: Option<Vec<Uuid>>,
    ) -> Result<Vec<User>, DbErr> {
        let base_query = self.build_base_query(restricted_user_ids);

        let users = base_query
            .paginate(self.db.as_ref(), pagination_meta.per_page)
            .fetch_page(pagination_meta.current_page)
            .await?;

        self.assemble_users(users).await
    }

    pub async fn get(&self, id: Uuid) -> Result<User, DbErr> {
        let persisted_user = user::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("User with such ID has not been found".to_string())
            })?;

        let mut assembled = self.assemble_users(vec![persisted_user]).await?;

        assembled
            .pop()
            .ok_or_else(|| DbErr::Custom("Failed to map single user".to_string()))
    }
}
