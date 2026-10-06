use std::sync::Arc;

use client::domain::state::connectivity::{ConnectivityState, ConnectivityStatus};
use client::domain::state::facade_events::{FacadeEvent, bridge_connectivity, facade_event_bus};

#[test]
fn a_subscriber_receives_a_published_event() {
    let bus = facade_event_bus();
    let mut receiver = bus.subscribe();

    bus.send(FacadeEvent::ConnectivityChanged(
        ConnectivityStatus::Offline,
    ))
    .unwrap();

    let event = receiver.try_recv().unwrap();
    assert!(matches!(
        event,
        FacadeEvent::ConnectivityChanged(ConnectivityStatus::Offline)
    ));
}

#[test]
fn sending_with_no_subscribers_does_not_panic() {
    let bus = facade_event_bus();
    assert!(
        bus.send(FacadeEvent::ConnectivityChanged(ConnectivityStatus::Online))
            .is_err()
    );
}

#[tokio::test]
async fn bridge_forwards_connectivity_changes_onto_the_bus() {
    let (_, rx) = tokio::sync::watch::channel(false);

    let connectivity = Arc::new(ConnectivityState::default());
    let bus = facade_event_bus();
    let mut receiver = bus.subscribe();

    let handle = tokio::spawn(bridge_connectivity(connectivity.clone(), bus, rx));

    connectivity.set_offline();

    let event = receiver.recv().await.unwrap();
    assert!(matches!(
        event,
        FacadeEvent::ConnectivityChanged(ConnectivityStatus::Offline)
    ));

    handle.abort();
}
