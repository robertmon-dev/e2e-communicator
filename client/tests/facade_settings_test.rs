use client::core::initializers::settings::FacadeSettings;

#[test]
fn facade_settings_default_to_no_socket_path_override() {
    let settings = FacadeSettings::default();
    assert!(settings.socket_path.is_none());
}

#[test]
fn facade_settings_deserialize_a_socket_path_override() {
    let settings: FacadeSettings =
        serde_json::from_str(r#"{"socket_path": "/tmp/custom.sock"}"#).unwrap();
    assert_eq!(settings.socket_path, Some("/tmp/custom.sock".to_string()));
}
