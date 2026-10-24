use chrono::{DateTime, Utc};
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

pub struct SessionState {
    sender: watch::Sender<Option<SessionToken>>,
}

impl SessionState {
    pub fn new() -> Self {
        let (sender, _receiver) = watch::channel(None);
        Self { sender }
    }

    pub fn get(&self) -> Option<SessionToken> {
        self.sender.borrow().clone()
    }

    pub fn is_authenticated(&self) -> bool {
        self.sender.borrow().is_some()
    }

    pub fn set(&self, token: SessionToken) {
        self.sender.send_replace(Some(token));
    }

    pub fn clear(&self) {
        self.sender.send_if_modified(|current| {
            let changed = current.is_some();
            *current = None;
            changed
        });
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<SessionToken>> {
        self.sender.subscribe()
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}
