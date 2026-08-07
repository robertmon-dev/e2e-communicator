use chrono::Utc;
use sea_orm::{ActiveModelTrait, DbErr, EntityTrait, IntoActiveModel, Set};

use super::UserRepository;
use super::utils::UserUpdateInput;
use crate::adapters::out::persistance::entities::user;
use crate::domain::dto::user::User;

pub trait Update {
    async fn update_user(&self, update_input: UserUpdateInput) -> Result<User, DbErr>;
}

impl Update for UserRepository {
    async fn update_user(&self, update_input: UserUpdateInput) -> Result<User, DbErr> {
        let persisted_user = user::Entity::find_by_id(update_input.id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("No user persisted with such ID".to_string()))?;

        let mut user_active = persisted_user.into_active_model();

        if let Some(name) = update_input.name {
            user_active.name = Set(Some(name));
        }

        if let Some(surname) = update_input.surname {
            user_active.surname = Set(Some(surname));
        }

        if let Some(nickname) = update_input.nickname {
            user_active.nickname = Set(nickname);
        }

        if let Some(phone) = update_input.phone {
            user_active.phone = Set(Some(phone));
        }

        user_active.updated_at = Set(Utc::now());

        user_active.update(self.db.as_ref()).await?;

        self.get(update_input.id).await
    }
}
