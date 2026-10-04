use client::domain::dto::outbox::OutboxStatus;

#[test]
fn outbox_status_round_trips_through_i32() {
    let statuses = [
        OutboxStatus::Pending,
        OutboxStatus::Sent,
        OutboxStatus::Failed,
        OutboxStatus::Delivered,
    ];

    for status in statuses {
        let raw: i32 = status.into();
        let parsed = OutboxStatus::try_from(raw).unwrap();
        assert_eq!(parsed, status);
    }
}
