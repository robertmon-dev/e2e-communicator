use sea_orm::DbErr;
use uuid::Uuid;

use crate::client::adapters::out::persistance::entities::message;
use crate::client::adapters::out::persistance::repositories::message::MessageRepository;
use crate::client::domain::dto::message::{Message, MessageContent};

pub trait Update {
    fn add_history_record(
        message_id: Uuid,
        message_history_record: MessageContent,
    ) -> Result<Message, DbErr>;
    fn update_message(message_id: Uuid, message: Message) -> Result<Message, DbErr>;
    fn update_messages(
        message_ids: Vec<Uuid>,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>, DbErr>;
}

impl Update for MessageRepository {
    fn add_history_record(
        message_id: Uuid,
        message_history_record: MessageContent,
    ) -> Result<Message, DbErr> {
        Err(DbErr::RecordNotInserted)
    }
    fn update_message(message_id: Uuid, message: Message) -> Result<Message, DbErr> {
        Err(DbErr::RecordNotUpdated)
    }
    fn update_messages(
        message_ids: Vec<Uuid>,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>, DbErr> {
        Err(DbErr::RecordNotUpdated)
    }
}
