use common::identity::{DevicePublicKeys, SIGNATURE_LEN};
use tonic::Status;
use uuid::Uuid;

use crate::adapters::r#in::grpc::proto;
use crate::adapters::r#in::grpc::proto::identity as identity_proto;
use crate::domain::dto::errors::{MailboxError, RegistryError};
use crate::domain::services::registry::RegisteredDevice;

pub fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|_| Status::invalid_argument(format!("invalid {field}")))
}

pub fn mailbox_status(err: MailboxError, operation: &str) -> Status {
    tracing::error!(error = %err, operation, "mailbox backend failure");
    Status::unavailable("mailbox unavailable")
}

pub fn registry_status(err: RegistryError) -> Status {
    match err {
        RegistryError::UserTaken => {
            Status::already_exists("user id is registered to another device")
        }
        RegistryError::DeviceTaken => {
            Status::already_exists("device id is registered with different keys")
        }
        RegistryError::InvalidProof
        | RegistryError::InvalidSignature
        | RegistryError::ChallengeMissing
        | RegistryError::UnknownDevice => {
            tracing::warn!(error = %err, "authentication failed");
            Status::unauthenticated("authentication failed")
        }
        RegistryError::LinkTokenInvalid => {
            Status::not_found("link token is unknown, used, or expired")
        }
        RegistryError::Identity(ref inner) => Status::invalid_argument(inner.to_string()),
        RegistryError::Corrupt(_) => {
            tracing::error!(error = %err, "registry data is corrupt");
            Status::internal("internal error")
        }
        RegistryError::Redis(_) | RegistryError::Store(_) => {
            tracing::error!(error = %err, "registry backend failure");
            Status::unavailable("registry unavailable")
        }
    }
}

pub fn parse_device_keys(
    keys: Option<identity_proto::DeviceKeys>,
) -> Result<DevicePublicKeys, Status> {
    let keys = keys.ok_or_else(|| Status::invalid_argument("missing keys"))?;
    DevicePublicKeys::from_slices(&keys.signing_key, &keys.agreement_key)
        .map_err(|_| Status::invalid_argument("keys must be 32 bytes each"))
}

pub fn parse_signature(raw: &[u8], field: &str) -> Result<[u8; SIGNATURE_LEN], Status> {
    raw.try_into()
        .map_err(|_| Status::invalid_argument(format!("{field} must be 64 bytes")))
}

pub fn device_to_proto(device: RegisteredDevice) -> identity_proto::RegisteredDevice {
    identity_proto::RegisteredDevice {
        device_id: device.device_id.to_string(),
        keys: Some(identity_proto::DeviceKeys {
            signing_key: device.keys.signing_key.to_vec(),
            agreement_key: device.keys.agreement_key.to_vec(),
        }),
        registered_at_unix_ms: device.registered_at.timestamp_millis(),
    }
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
