use sea_orm::{DbErr, EntityTrait};
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::user;
use crate::client::adapters::out::persistance::repositories::user::UserRepository;
use crate::client::domain::dto::user::User;

pub trait Remove {
    async fn remove_user(&self, id: Uuid) -> Result<User, DbErr>;
}

impl Remove for UserRepository {
    async fn remove_user(&self, id: Uuid) -> Result<User, DbErr> {
        let user = self.get(id).await?;

        let delete_result = user::Entity::delete_by_id(id)
            .exec(self.db.as_ref())
            .await?;

        if delete_result.rows_affected == 0 {
            return Err(DbErr::RecordNotUpdated);
        }

        Ok(user)
    }
}
