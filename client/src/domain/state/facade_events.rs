use std::sync::Arc;

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::domain::dto::message::Message;
use crate::domain::state::connectivity::{ConnectivityState, ConnectivityStatus};

#[derive(Debug, Clone)]
pub enum FacadeEvent {
    MessageReceived(Message),
    DeliveryReceiptMarked {
        message_id: Uuid,
        recipient_id: Uuid,
    },
    ConnectivityChanged(ConnectivityStatus),
}

pub type FacadeEventBus = broadcast::Sender<FacadeEvent>;

pub fn facade_event_bus() -> FacadeEventBus {
    let (sender, _receiver) = broadcast::channel(256);
    sender
}

pub async fn bridge_connectivity(connectivity: Arc<ConnectivityState>, events: FacadeEventBus) {
    let mut receiver = connectivity.subscribe();
    let _ = events.send(FacadeEvent::ConnectivityChanged(*receiver.borrow()));
    loop {
        if receiver.changed().await.is_err() {
            break;
        }
        let status = *receiver.borrow();
        let _ = events.send(FacadeEvent::ConnectivityChanged(status));
    }
}
