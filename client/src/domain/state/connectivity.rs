use serde::{Deserialize, Serialize};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectivityStatus {
    Online,
    Offline,
}

pub struct ConnectivityState {
    sender: watch::Sender<ConnectivityStatus>,
}

impl ConnectivityState {
    pub fn new(initial: ConnectivityStatus) -> Self {
        let (sender, _receiver) = watch::channel(initial);
        Self { sender }
    }

    pub fn status(&self) -> ConnectivityStatus {
        *self.sender.borrow()
    }

    pub fn is_online(&self) -> bool {
        self.status() == ConnectivityStatus::Online
    }

    pub fn set_online(&self) {
        self.sender.send_if_modified(|status| {
            let changed = *status != ConnectivityStatus::Online;
            *status = ConnectivityStatus::Online;
            changed
        });
    }

    pub fn set_offline(&self) {
        self.sender.send_if_modified(|status| {
            let changed = *status != ConnectivityStatus::Offline;
            *status = ConnectivityStatus::Offline;
            changed
        });
    }

    pub fn subscribe(&self) -> watch::Receiver<ConnectivityStatus> {
        self.sender.subscribe()
    }
}

impl Default for ConnectivityState {
    fn default() -> Self {
        Self::new(ConnectivityStatus::Online)
    }
}
