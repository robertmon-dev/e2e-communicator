use std::sync::Arc;

use crate::domain::state::connectivity::ConnectivityState;

#[derive(uniffi::Object)]
pub struct ConnectivityBridge {
    state: Arc<ConnectivityState>,
}

#[uniffi::export]
impl ConnectivityBridge {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(ConnectivityState::default()),
        })
    }

    pub fn set_online(&self, online: bool) {
        if online {
            self.state.set_online();
        } else {
            self.state.set_offline();
        }
    }

    pub fn is_online(&self) -> bool {
        self.state.is_online()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_online() {
        let bridge = ConnectivityBridge::new();
        assert!(bridge.is_online());
    }

    #[test]
    fn reflects_pushed_state() {
        let bridge = ConnectivityBridge::new();

        bridge.set_online(false);
        assert!(!bridge.is_online());

        bridge.set_online(true);
        assert!(bridge.is_online());
    }
}
