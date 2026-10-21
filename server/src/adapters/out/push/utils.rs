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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stringify_data_converts_non_string_values_to_strings() {
        let data = json!({ "count": 3, "label": "hello", "flag": true });
        let result = stringify_data(data).unwrap();

        assert_eq!(
            result.get("count").unwrap(),
            &Value::String("3".to_string())
        );
        assert_eq!(
            result.get("label").unwrap(),
            &Value::String("hello".to_string())
        );
        assert_eq!(
            result.get("flag").unwrap(),
            &Value::String("true".to_string())
        );
    }

    #[test]
    fn stringify_data_returns_none_for_non_object() {
        let data = json!([1, 2, 3]);
        assert!(stringify_data(data).is_none());
    }

    #[test]
    fn is_unregistered_true_for_404() {
        assert!(is_unregistered(StatusCode::NOT_FOUND, ""));
    }

    #[test]
    fn is_unregistered_true_for_unregistered_error_code() {
        let body = json!({
            "error": { "details": [ { "errorCode": "UNREGISTERED" } ] }
        })
        .to_string();

        assert!(is_unregistered(StatusCode::BAD_REQUEST, &body));
    }

    #[test]
    fn is_unregistered_false_for_other_errors() {
        let body = json!({
            "error": { "details": [ { "errorCode": "INVALID_ARGUMENT" } ] }
        })
        .to_string();

        assert!(!is_unregistered(StatusCode::BAD_REQUEST, &body));
    }
}
