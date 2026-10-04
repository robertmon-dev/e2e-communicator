use client::domain::state::connectivity::ConnectivityStatus;

#[test]
fn connectivity_status_round_trips_through_json() {
    let json = serde_json::to_string(&ConnectivityStatus::Offline).unwrap();
    let restored: ConnectivityStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, ConnectivityStatus::Offline);
}
