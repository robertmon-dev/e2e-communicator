use tokio::sync::watch;

use crate::domain::dto::local_identity::LocalIdentity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityStatus {
    Unregistered,
    Registered(LocalIdentity),
    Conflict(LocalIdentity),
}

impl IdentityStatus {
    pub fn label(&self) -> &'static str {
        match self {
            IdentityStatus::Unregistered => "unregistered",
            IdentityStatus::Registered(_) => "registered",
            IdentityStatus::Conflict(_) => "conflict",
        }
    }

    pub fn identity(&self) -> Option<&LocalIdentity> {
        match self {
            IdentityStatus::Unregistered => None,
            IdentityStatus::Registered(identity) | IdentityStatus::Conflict(identity) => {
                Some(identity)
            }
        }
    }
}

pub struct IdentityState {
    sender: watch::Sender<IdentityStatus>,
}

impl IdentityState {
    pub fn new(initial: IdentityStatus) -> Self {
        let (sender, _receiver) = watch::channel(initial);
        Self { sender }
    }

    pub fn current(&self) -> IdentityStatus {
        self.sender.borrow().clone()
    }

    pub fn registered(&self) -> Option<LocalIdentity> {
        match &*self.sender.borrow() {
            IdentityStatus::Registered(identity) => Some(identity.clone()),
            _ => None,
        }
    }

    pub fn set(&self, status: IdentityStatus) {
        self.sender.send_replace(status);
    }

    pub fn subscribe(&self) -> watch::Receiver<IdentityStatus> {
        self.sender.subscribe()
    }
}
