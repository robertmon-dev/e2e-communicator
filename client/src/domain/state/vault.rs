use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultStatus {
    Locked,
    Unlocked,
}

impl VaultStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::Unlocked => "unlocked",
        }
    }

    pub fn is_unlocked(self) -> bool {
        matches!(self, Self::Unlocked)
    }
}

pub struct VaultState {
    sender: watch::Sender<VaultStatus>,
}

impl Default for VaultState {
    fn default() -> Self {
        Self {
            sender: watch::channel(VaultStatus::Locked).0,
        }
    }
}

impl VaultState {
    pub fn current(&self) -> VaultStatus {
        *self.sender.borrow()
    }

    pub fn is_unlocked(&self) -> bool {
        self.current().is_unlocked()
    }

    pub fn unlocked() -> Self {
        Self {
            sender: watch::channel(VaultStatus::Unlocked).0,
        }
    }

    pub fn set(&self, status: VaultStatus) {
        self.sender.send_replace(status);
    }

    pub fn subscribe(&self) -> watch::Receiver<VaultStatus> {
        self.sender.subscribe()
    }
}
