use std::sync::Arc;

use tokio::sync::watch::Receiver;

use crate::domain::state::facade_events::FacadeEventBus;

pub struct FcmClient {}

impl FcmClient {
    pub fn new() -> Self {
        Self {}
    }
}

pub async fn run(fcm_client: Arc<FcmClient>, events: FacadeEventBus, mut shutdown: Receiver<bool>) {
    let mut rx = events.subscribe();
    tracing::info!("FCM disptach daemon intialized!");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("FCM daemon received shutdown signal, exiting...");
                    break;
                }
            }
        }
    }
}
