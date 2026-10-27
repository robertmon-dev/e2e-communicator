use chrono::Utc;
use uuid::Uuid;

use client::domain::dto::message::{MessageContent, MessageWirePayload};
use common::envelope::{EncryptedEnvelope, EventType};

#[test]
fn message_wire_payload_round_trips_through_json() {
    let payload = MessageWirePayload {
        message_id: Uuid::new_v4(),
        reply_to_id: Some(Uuid::new_v4()),
        available_to: Some(Utc::now()),
        content: MessageContent {
            id: Uuid::new_v4(),
            text: "hello".to_string(),
            attachments: Vec::new(),
            reactions: Vec::new(),
        },
    };

    let serialized = serde_json::to_vec(&payload).unwrap();
    let deserialized: MessageWirePayload = serde_json::from_slice(&serialized).unwrap();

    assert_eq!(deserialized.message_id, payload.message_id);
    assert_eq!(deserialized.reply_to_id, payload.reply_to_id);
    assert_eq!(
        deserialized.available_to.map(|d| d.timestamp_millis()),
        payload.available_to.map(|d| d.timestamp_millis())
    );
    assert_eq!(deserialized.content.text, payload.content.text);
}

#[test]
fn new_event_type_discriminants_round_trip_through_try_from() {
    assert_eq!(
        EventType::try_from(7).unwrap(),
        EventType::InvitationResponse
    );
    assert_eq!(EventType::try_from(8).unwrap(), EventType::MemberAdded);
    assert_eq!(
        EventType::try_from(9).unwrap(),
        EventType::ChatKeyAnnouncement
    );
}

#[test]
fn an_out_of_range_event_type_discriminant_is_rejected() {
    assert!(EventType::try_from(10).is_err());
}

#[test]
fn encrypted_envelope_round_trips_with_a_recipient_device_id() {
    let envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id: Uuid::new_v4(),
        sender_id: Uuid::new_v4(),
        sender_device_id: Uuid::new_v4(),
        recipient_id: Uuid::new_v4(),
        recipient_device_id: Some(Uuid::new_v4()),
        created_at: Utc::now(),
        event_type: EventType::Message,
        notify: true,
        ciphertext: vec![1, 2, 3],
    };

    let serialized = serde_json::to_vec(&envelope).unwrap();
    let deserialized: EncryptedEnvelope = serde_json::from_slice(&serialized).unwrap();

    assert_eq!(deserialized.id, envelope.id);
    assert_eq!(deserialized.sender_device_id, envelope.sender_device_id);
    assert_eq!(
        deserialized.recipient_device_id,
        envelope.recipient_device_id
    );
    assert_eq!(deserialized.notify, envelope.notify);
}
