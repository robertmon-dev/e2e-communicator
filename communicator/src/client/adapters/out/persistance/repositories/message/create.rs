use sea_orm::DbErr;

use crate::client::{
    adapters::out::persistance::repositories::message::{MessageRepository, utils::CreateMessage},
    domain::dto::message::Message,
};

pub trait Create {
    async fn create(&self, create_input: CreateMessage) -> Result<Message, DbErr>;
}

impl Create for MessageRepository {
    async fn create(&self, create_input: CreateMessage) -> Result<Message, DbErr> {
        Err(DbErr::RecordNotInserted)
    }
}
