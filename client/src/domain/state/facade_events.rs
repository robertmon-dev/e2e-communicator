use std::sync::Arc;

use tokio::sync::{self, broadcast};
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

pub async fn bridge_connectivity(
    connectivity: Arc<ConnectivityState>,
    events: FacadeEventBus,
    mut shutdown: sync::watch::Receiver<bool>,
) {
    if *shutdown.borrow() {
        return;
    }

    let mut receiver = connectivity.subscribe();

    let _ = events.send(FacadeEvent::ConnectivityChanged(
        *receiver.borrow_and_update(),
    ));

    loop {
        tokio::select! {
            res = shutdown.changed() => {
                if res.is_err() || *shutdown.borrow() {
                    tracing::info!("Connectivity bridge received shutdown signal, exiting...");
                    break;
                }
            }

            res = receiver.changed() => {
                match res {
                    Ok(()) => {
                        let status = *receiver.borrow_and_update();
                        let _ = events.send(FacadeEvent::ConnectivityChanged(status));
                    }
                    Err(_) => {
                        tracing::warn!("Connectivity state sender dropped, terminating bridge");
                        break;
                    }
                }
            }
        }
    }
}
