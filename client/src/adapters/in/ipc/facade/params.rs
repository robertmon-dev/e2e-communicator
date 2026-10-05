use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    adapters::out::persistance::repositories::attachment::utils::AttachmentRestrictions,
    domain::dto::{
        filter_attachments::ListAttachmentsFilter,
        filter_chat::ListChatsFilter,
        filter_messages::ListMessagesFilter,
        message::{Attachment, MessageContent},
        utils::{PaginationMeta, TimeStep},
    },
};

#[derive(Deserialize)]
pub struct SessionInitParams {
    pub protocol_version: u32,
    pub token: Option<String>,
}

#[derive(Serialize)]
pub struct SessionInitResponse {
    pub protocol_version: u32,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub connectivity: String,
}

#[derive(Deserialize)]
pub struct ChatsListParams {
    filter: ListChatsFilter,
    pagination: PaginationMeta,
}

#[derive(Deserialize)]
pub struct ChatsGetParams {
    chat_id: Uuid,
}

#[derive(Deserialize)]
pub struct MessagesListParams {
    filter: ListMessagesFilter,
    pagination: PaginationMeta,
}

#[derive(Deserialize)]
pub struct SendMessageParams {
    chat_id: Uuid,
    text: String,
    reply_to_id: Option<Uuid>,
    available_for: Option<TimeStep>,
    #[serde(default)]
    attachments: Vec<Attachment>,
    scheduled_for: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct MessagesGetParams {
    message_id: Uuid,
}

#[derive(Deserialize)]
pub struct MessagesDeleteParams {
    message_id: Uuid,
}

#[derive(Deserialize)]
pub struct MessagesMarkDeliveredParams {
    message_id: Uuid,
}

#[derive(Deserialize)]
pub struct AttachmentGetParams {
    attachment_id: Uuid,
}

#[derive(Deserialize)]
pub struct AttachmentListParams {
    filters: ListAttachmentsFilter,
    pagination: PaginationMeta,
    restrictions: AttachmentRestrictions,
}

#[derive(Deserialize)]
pub struct AttachmentDeleteParams {
    attachment_id: Uuid,
}

#[derive(Deserialize)]
pub struct AttachmentDeleteManyParams {
    attachment_ids: Vec<Uuid>,
}

#[derive(Deserialize)]
pub struct MessageUpdateParams {
    message_id: Uuid,
    message_history_record: MessageContent,
}
