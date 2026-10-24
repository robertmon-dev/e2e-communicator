use chrono::Utc;
use client::domain::dto::local_identity::LocalIdentity;
use client::domain::state::identity::{IdentityState, IdentityStatus};
use uuid::Uuid;

fn identity() -> LocalIdentity {
    LocalIdentity {
        user_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        agreement_public_key: vec![1; 32],
        signing_public_key: vec![2; 32],
        created_at: Utc::now(),
    }
}

#[test]
fn labels_match_the_facade_contract() {
    assert_eq!(IdentityStatus::Unregistered.label(), "unregistered");
    assert_eq!(IdentityStatus::Registered(identity()).label(), "registered");
    assert_eq!(IdentityStatus::Conflict(identity()).label(), "conflict");
}

#[test]
fn registered_returns_the_identity_only_when_registered() {
    let id = identity();

    assert!(
        IdentityState::new(IdentityStatus::Unregistered)
            .registered()
            .is_none()
    );
    assert_eq!(
        IdentityState::new(IdentityStatus::Registered(id.clone())).registered(),
        Some(id.clone())
    );
    assert!(
        IdentityState::new(IdentityStatus::Conflict(id))
            .registered()
            .is_none()
    );
}

#[tokio::test]
async fn set_notifies_subscribers() {
    let state = IdentityState::new(IdentityStatus::Unregistered);
    let mut rx = state.subscribe();
    let id = identity();

    state.set(IdentityStatus::Registered(id.clone()));

    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), IdentityStatus::Registered(id));
}
