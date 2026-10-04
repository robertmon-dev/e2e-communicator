use std::sync::Arc;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use sea_orm_migration::MigratorTrait;
use uuid::Uuid;

use client::adapters::out::persistance::entities::{chat, message, user};
use client::domain::dto::errors::TransportError;
use client::domain::services::inbox::{EnvelopeSubscription, InboundStream};

pub struct NoopSubscription;

impl EnvelopeSubscription for NoopSubscription {
    async fn subscribe(
        &self,
        _user_id: Uuid,
        _device_id: Uuid,
    ) -> Result<InboundStream, TransportError> {
        unimplemented!("not exercised by these tests")
    }

    async fn ack(
        &self,
        _user_id: Uuid,
        _device_id: Uuid,
        _delivery_token: &str,
    ) -> Result<(), TransportError> {
        Ok(())
    }
}

pub async fn setup_db() -> Arc<sea_orm::DatabaseConnection> {
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    migration::Migrator::up(&db, None).await.unwrap();
    Arc::new(db)
}

pub async fn seed_user(db: &sea_orm::DatabaseConnection, id: Uuid) {
    let now = Utc::now();
    user::ActiveModel {
        id: Set(id),
        name: Set(None),
        surname: Set(None),
        nickname: Set(id.to_string()),
        phone: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
}

pub async fn seed_chat(db: &sea_orm::DatabaseConnection, id: Uuid) {
    let now = Utc::now();
    chat::ActiveModel {
        id: Set(id),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
}

pub async fn seed_message(
    db: &sea_orm::DatabaseConnection,
    id: Uuid,
    author_id: Uuid,
    chat_id: Uuid,
) {
    let now = Utc::now();
    message::ActiveModel {
        id: Set(id),
        author_id: Set(author_id),
        chat_id: Set(chat_id),
        available_to: Set(None),
        hidden: Set(false),
        is_reply: Set(false),
        reply_to_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();
}
