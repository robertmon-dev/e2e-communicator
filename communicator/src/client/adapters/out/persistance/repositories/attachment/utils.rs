use uuid::Uuid;

pub struct AttachmentRestrictions {
    pub restricted_chat_ids: Option<Vec<Uuid>>,
    pub restricted_user_ids: Option<Vec<Uuid>>,
    pub restricted_message_ids: Option<Vec<Uuid>>,
    pub restricted_message_history_ids: Option<Vec<Uuid>>,
    pub restricted_metadata_ids: Option<Vec<Uuid>>,
}
