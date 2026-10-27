use uuid::Uuid;

pub(super) const SESSION_PREFIX: &str = "session:";

pub(super) fn user_devices_key(user_id: Uuid) -> String {
    format!("identity:user:{user_id}")
}

pub(super) fn device_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("identity:device:{user_id}:{device_id}")
}

pub(super) fn user_key_key(user_id: Uuid) -> String {
    format!("identity:user:{user_id}:key")
}

pub(super) fn link_key(token: &str) -> String {
    format!("link:{token}")
}

pub(super) fn challenge_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("challenge:{user_id}:{device_id}")
}

pub(super) fn session_key(token: &str) -> String {
    format!("{SESSION_PREFIX}{token}")
}

pub(super) fn device_sessions_key(user_id: Uuid, device_id: Uuid) -> String {
    format!("sessions:{user_id}:{device_id}")
}
