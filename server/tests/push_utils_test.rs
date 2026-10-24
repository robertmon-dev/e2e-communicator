use reqwest::StatusCode;
use serde_json::{Value, json};
use server::adapters::out::push::utils::{is_unregistered, stringify_data};

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
