use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    adapters::out::persistance::repositories::{
        attachment::utils::AttachmentRestrictions, user::utils::UserUpdateInput,
    },
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

#[derive(Deserialize)]
pub struct VaultUnlockParams {
    pub pin: String,
}

#[derive(Serialize)]
pub struct SessionInitResponse {
    pub protocol_version: u32,
    pub vault_state: String,
    pub identity_state: String,
    pub user_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub connectivity: String,
    pub authenticated: bool,
}

#[derive(Deserialize)]
pub struct IdentityLinkParams {
    pub user_id: Uuid,
    pub token: String,
    pub user_seed: String,
}

#[derive(Deserialize)]
pub struct DevicesRevokeParams {
    pub device_id: Uuid,
}

#[derive(Deserialize)]
pub struct ChatsListParams {
    pub filter: ListChatsFilter,
    pub pagination: PaginationMeta,
}

#[derive(Deserialize)]
pub struct ChatsGetParams {
    pub chat_id: Uuid,
}

#[derive(Deserialize)]
pub struct MessagesListParams {
    pub filter: ListMessagesFilter,
    pub pagination: PaginationMeta,
}

#[derive(Deserialize)]
pub struct SendMessageParams {
    pub chat_id: Uuid,
    pub text: String,
    pub reply_to_id: Option<Uuid>,
    pub available_for: Option<TimeStep>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub scheduled_for: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct MessagesGetParams {
    pub message_id: Uuid,
}

#[derive(Deserialize)]
pub struct MessagesDeleteParams {
    pub message_id: Uuid,
}

#[derive(Deserialize)]
pub struct MessagesMarkDeliveredParams {
    pub message_id: Uuid,
}

#[derive(Deserialize)]
pub struct AttachmentGetParams {
    pub attachment_id: Uuid,
}

#[derive(Deserialize)]
pub struct AttachmentListParams {
    pub filters: ListAttachmentsFilter,
    pub pagination: PaginationMeta,
    pub restrictions: AttachmentRestrictions,
}

#[derive(Deserialize)]
pub struct AttachmentDeleteParams {
    pub attachment_id: Uuid,
}

#[derive(Deserialize)]
pub struct AttachmentDeleteManyParams {
    pub attachment_ids: Vec<Uuid>,
}

#[derive(Deserialize)]
pub struct MessageUpdateParams {
    pub message_id: Uuid,
    pub message_history_record: MessageContent,
}

#[derive(Deserialize)]
pub struct ChatsDeleteManyParams {
    pub chat_ids: Vec<Uuid>,
}

#[derive(Deserialize)]
pub struct ChatsDeleteHistoryRecord {
    pub history_record_id: Uuid,
}

#[derive(Deserialize)]
pub struct UsersGetParams {
    pub user_id: Uuid,
}

#[derive(Deserialize)]
pub struct UsersListParams {
    pub pagination: PaginationMeta,
    pub restrictions: Option<Vec<Uuid>>,
}

#[derive(Deserialize)]
pub struct UsersDeleteParams {
    pub user_id: Uuid,
}

#[derive(Deserialize)]
pub struct UsersUpdateParams {
    pub user_id: Uuid,
    pub input: UserUpdateInput,
}

#[derive(Deserialize)]
pub struct InvitationsCreateParams {
    pub code: String,
    pub label: String,
    #[serde(default)]
    pub chat_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct InvitationsListParams {
    #[serde(default)]
    pub status: Option<u8>,
    #[serde(default)]
    pub direction: Option<u8>,
}

#[derive(Deserialize)]
pub struct InvitationsAcceptParams {
    pub invitation_id: Uuid,
}

#[derive(Deserialize)]
pub struct InvitationsRejectParams {
    pub invitation_id: Uuid,
}

#[derive(Deserialize)]
pub struct InvitationsCancelParams {
    pub invitation_id: Uuid,
}

#[derive(Deserialize)]
pub struct ChatsCreateGroupParams {
    pub name: String,
}

#[derive(Deserialize)]
pub struct UsersSafetyNumberParams {
    pub user_id: Uuid,
}

#[derive(Deserialize)]
pub struct UsersVerifyParams {
    pub user_id: Uuid,
}
