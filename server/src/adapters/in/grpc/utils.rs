use tonic::Status;
use uuid::Uuid;

use crate::adapters::r#in::grpc::proto;
use crate::domain::dto::errors::MailboxError;

pub fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|_| Status::invalid_argument(format!("invalid {field}")))
}

pub fn mailbox_status(err: MailboxError) -> Status {
    Status::unavailable(err.to_string())
}

pub fn envelope_from_proto(
    proto_envelope: proto::EncryptedEnvelope,
) -> Result<common::envelope::EncryptedEnvelope, Status> {
    let event_type = common::envelope::EventType::try_from(proto_envelope.event_type)
        .map_err(|_| Status::invalid_argument("invalid event_type"))?;

    let created_at = chrono::DateTime::from_timestamp_millis(proto_envelope.created_at_unix_ms)
        .ok_or_else(|| Status::invalid_argument("invalid created_at_unix_ms"))?;

    Ok(common::envelope::EncryptedEnvelope {
        id: parse_uuid(&proto_envelope.id, "id")?,
        chat_id: parse_uuid(&proto_envelope.chat_id, "chat_id")?,
        sender_id: parse_uuid(&proto_envelope.sender_id, "sender_id")?,
        recipient_id: parse_uuid(&proto_envelope.recipient_id, "recipient_id")?,
        created_at,
        event_type,
        ciphertext: proto_envelope.ciphertext,
    })
}

pub fn envelope_to_proto(
    envelope: common::envelope::EncryptedEnvelope,
) -> proto::EncryptedEnvelope {
    proto::EncryptedEnvelope {
        id: envelope.id.to_string(),
        chat_id: envelope.chat_id.to_string(),
        sender_id: envelope.sender_id.to_string(),
        recipient_id: envelope.recipient_id.to_string(),
        created_at_unix_ms: envelope.created_at.timestamp_millis(),
        event_type: envelope.event_type as i32,
        ciphertext: envelope.ciphertext,
    }
}
