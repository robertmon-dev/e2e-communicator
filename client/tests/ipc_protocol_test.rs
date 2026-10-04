use client::adapters::r#in::ipc::protocol::{Notification, Outcome, Request, Response, RpcError};
use client::domain::state::facade_events::FacadeEvent;
use serde_json::Value;

#[test]
fn ok_outcome_serializes_without_an_error_field() {
    let response = Response {
        id: Value::from(1),
        outcome: Outcome::Ok {
            result: Value::from("done"),
        },
    };
    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["result"], "done");
    assert!(json.get("error").is_none());
}

#[test]
fn err_outcome_serializes_without_a_result_field() {
    let response = Response {
        id: Value::from(1),
        outcome: Outcome::Err {
            error: RpcError {
                code: -32000,
                message: "boom".to_string(),
            },
        },
    };
    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["error"]["code"], -32000);
    assert!(json.get("result").is_none());
}

#[test]
fn request_deserializes_from_a_json_line() {
    let request: Request =
        serde_json::from_str(r#"{"id": 1, "method": "identity.get", "params": {}}"#).unwrap();
    assert_eq!(request.method, "identity.get");
    assert_eq!(request.id, Value::from(1));
}

#[test]
fn a_delivery_receipt_event_becomes_a_message_delivered_notification() {
    let message_id = uuid::Uuid::new_v4();
    let recipient_id = uuid::Uuid::new_v4();
    let notification: Notification = FacadeEvent::DeliveryReceiptMarked {
        message_id,
        recipient_id,
    }
    .into();

    assert_eq!(notification.method, "message.delivered");
    assert_eq!(notification.params["message_id"], message_id.to_string());
    assert_eq!(
        notification.params["recipient_id"],
        recipient_id.to_string()
    );
}
