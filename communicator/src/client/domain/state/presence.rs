use std::collections::HashMap;
use std::sync::RwLock;

use tokio::sync::watch;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceStatus {
    Online,
    Offline,
}

pub struct PresenceState {
    statuses: RwLock<HashMap<Uuid, PresenceStatus>>,
    default_status: watch::Sender<PresenceStatus>,
}

impl PresenceState {
    pub fn new(default_status: PresenceStatus) -> Self {
        let (default_status, _receiver) = watch::channel(default_status);

        Self {
            statuses: RwLock::new(HashMap::new()),
            default_status,
        }
    }

    pub fn status_of(&self, user_id: Uuid) -> PresenceStatus {
        self.statuses
            .read()
            .expect("presence state lock poisoned")
            .get(&user_id)
            .copied()
            .unwrap_or_else(|| self.default_status())
    }

    pub fn set_status(&self, user_id: Uuid, status: PresenceStatus) {
        self.statuses
            .write()
            .expect("presence state lock poisoned")
            .insert(user_id, status);
    }

    pub fn clear_status(&self, user_id: Uuid) {
        self.statuses
            .write()
            .expect("presence state lock poisoned")
            .remove(&user_id);
    }

    pub fn default_status(&self) -> PresenceStatus {
        *self.default_status.borrow()
    }

    pub fn set_default_status(&self, status: PresenceStatus) {
        self.default_status.send_if_modified(|current| {
            let changed = *current != status;
            *current = status;
            changed
        });
    }

    pub fn subscribe_default(&self) -> watch::Receiver<PresenceStatus> {
        self.default_status.subscribe()
    }
}

impl Default for PresenceState {
    fn default() -> Self {
        Self::new(PresenceStatus::Offline)
    }
}
