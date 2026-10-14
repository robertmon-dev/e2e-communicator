use reqwest::StatusCode;
use serde_json::{Map, Value};

pub fn stringify_data(data: Value) -> Option<Map<String, Value>> {
    let Value::Object(map) = data else {
        tracing::warn!("FCM data payload is not a JSON object, skipping");
        return None;
    };
    Some(
        map.into_iter()
            .map(|(k, v)| {
                let v = match v {
                    Value::String(s) => Value::String(s),
                    other => Value::String(other.to_string()),
                };
                (k, v)
            })
            .collect(),
    )
}

pub fn is_unregistered(status: StatusCode, body: &str) -> bool {
    if status == StatusCode::NOT_FOUND {
        return true;
    }
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["details"].as_array().cloned())
        .is_some_and(|details| {
            details
                .iter()
                .any(|d| d["errorCode"].as_str() == Some("UNREGISTERED"))
        })
}
