use sea_orm::DbErr;
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::message;
use crate::client::adapters::out::persistance::repositories::message::MessageRepository;
use crate::client::domain::dto::message::{Message, MessageContent};

pub trait Remove {
    fn remove_history_record(
        message_id: Uuid,
        message_history_record_id: Uuid,
    ) -> Result<Message, DbErr>;
    fn remove_messages(message_ids: Vec<Uuid>) -> Result<Vec<Message>, DbErr>;
    fn remove_message(message_id: Uuid) -> Result<Message, DbErr>;
}

impl Remove for MessageRepository {
    fn remove_history_record(
        message_id: Uuid,
        message_history_record_id: Uuid,
    ) -> Result<Message, DbErr> {
        Err(DbErr::RecordNotInserted)
    }
    fn remove_message(message_id: Uuid) -> Result<Message, DbErr> {
        Err(DbErr::RecordNotUpdated)
    }
    fn remove_messages(message_ids: Vec<Uuid>) -> Result<Vec<Message>, DbErr> {
        Err(DbErr::RecordNotUpdated)
    }
}
