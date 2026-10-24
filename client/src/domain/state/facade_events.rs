use std::sync::Arc;

use tokio::sync::{self, broadcast};
use uuid::Uuid;

use crate::domain::dto::message::Message;
use crate::domain::state::connectivity::{ConnectivityState, ConnectivityStatus};
use crate::domain::state::identity::{IdentityState, IdentityStatus};
use crate::domain::state::session::SessionState;

#[derive(Debug, Clone)]
pub enum FacadeEvent {
    MessageReceived(Message),
    DeliveryReceiptMarked {
        message_id: Uuid,
        recipient_id: Uuid,
    },
    ConnectivityChanged(ConnectivityStatus),
    IdentityChanged(IdentityStatus),
    SessionChanged {
        authenticated: bool,
    },
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

pub async fn bridge_identity(
    identity: Arc<IdentityState>,
    session: Arc<SessionState>,
    events: FacadeEventBus,
    mut shutdown: sync::watch::Receiver<bool>,
) {
    if *shutdown.borrow() {
        return;
    }

    let mut identity_rx = identity.subscribe();
    let mut session_rx = session.subscribe();

    let _ = events.send(FacadeEvent::IdentityChanged(
        identity_rx.borrow_and_update().clone(),
    ));
    let _ = events.send(FacadeEvent::SessionChanged {
        authenticated: session_rx.borrow_and_update().is_some(),
    });

    loop {
        tokio::select! {
            res = shutdown.changed() => {
                if res.is_err() || *shutdown.borrow() {
                    tracing::info!("Identity bridge received shutdown signal, exiting...");
                    break;
                }
            }

            res = identity_rx.changed() => {
                match res {
                    Ok(()) => {
                        let status = identity_rx.borrow_and_update().clone();
                        let _ = events.send(FacadeEvent::IdentityChanged(status));
                    }
                    Err(_) => {
                        tracing::warn!("Identity state sender dropped, terminating bridge");
                        break;
                    }
                }
            }

            res = session_rx.changed() => {
                match res {
                    Ok(()) => {
                        let authenticated = session_rx.borrow_and_update().is_some();
                        let _ = events.send(FacadeEvent::SessionChanged { authenticated });
                    }
                    Err(_) => {
                        tracing::warn!("Session state sender dropped, terminating bridge");
                        break;
                    }
                }
            }
        }
    }
}
