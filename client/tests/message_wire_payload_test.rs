use chrono::Utc;
use uuid::Uuid;

use client::domain::dto::message::{MessageContent, MessageWirePayload};

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
