use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, Set,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::adapters::out::persistance::entities::invitation;
use crate::adapters::out::persistance::repositories::utils::{ConditionExt, safe_bulk_upsert};
use crate::domain::dto::invitation::{Invitation, InvitationDirection, InvitationStatus};

pub struct InvitationRepository {
    db: Arc<DatabaseConnection>,
}

impl InvitationRepository {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn upsert(&self, invitation: Invitation) -> Result<(), DbErr> {
        let active_model = invitation.into_model().into_active_model();

        safe_bulk_upsert::<invitation::Entity, _, _>(self.db.as_ref(), vec![active_model]).await
    }

    pub async fn get(&self, id: Uuid) -> Result<Invitation, DbErr> {
        let persisted = invitation::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Invitation with such ID has not been found".to_string())
            })?;

        Invitation::from(persisted)
            .map_err(|e| DbErr::Custom(format!("Failed to structurize invitation: {e}")))
    }

    pub async fn list(
        &self,
        status: Option<InvitationStatus>,
        direction: Option<InvitationDirection>,
    ) -> Result<Vec<Invitation>, DbErr> {
        let conditions = Condition::all()
            .add_opt(status, |s| invitation::Column::Status.eq(i32::from(s)))
            .add_opt(direction, |d| {
                invitation::Column::Direction.eq(i32::from(d))
            });

        let persisted = invitation::Entity::find()
            .filter(conditions)
            .order_by_desc(invitation::Column::CreatedAt)
            .all(self.db.as_ref())
            .await?;

        persisted
            .into_iter()
            .map(Invitation::from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DbErr::Custom(format!("Failed to structurize invitation: {e}")))
    }

    pub async fn resolve(&self, id: Uuid, status: InvitationStatus) -> Result<Invitation, DbErr> {
        let persisted = invitation::Entity::find_by_id(id)
            .one(self.db.as_ref())
            .await?
            .ok_or_else(|| {
                DbErr::RecordNotFound("Invitation with such ID has not been found".to_string())
            })?;

        let now = Utc::now();
        let mut active = persisted.into_active_model();
        active.status = Set(i32::from(status));
        active.updated_at = Set(now);
        active.responded_at = Set(Some(now));

        let updated = active.update(self.db.as_ref()).await?;

        Invitation::from(updated)
            .map_err(|e| DbErr::Custom(format!("Failed to structurize invitation: {e}")))
    }

    pub async fn find_pending_for_peer(
        &self,
        peer_user_id: Uuid,
    ) -> Result<Option<Invitation>, DbErr> {
        let persisted = invitation::Entity::find()
            .filter(invitation::Column::Status.eq(i32::from(InvitationStatus::Pending)))
            .filter(
                Condition::any()
                    .add(invitation::Column::InviteeUserId.eq(peer_user_id))
                    .add(invitation::Column::InviterUserId.eq(peer_user_id)),
            )
            .one(self.db.as_ref())
            .await?;

        persisted
            .map(Invitation::from)
            .transpose()
            .map_err(|e| DbErr::Custom(format!("Failed to structurize invitation: {e}")))
    }

    pub async fn find_by_chat(&self, chat_id: Uuid) -> Result<Option<Invitation>, DbErr> {
        let persisted = invitation::Entity::find()
            .filter(invitation::Column::ChatId.eq(chat_id))
            .one(self.db.as_ref())
            .await?;

        persisted
            .map(Invitation::from)
            .transpose()
            .map_err(|e| DbErr::Custom(format!("Failed to structurize invitation: {e}")))
    }
}
