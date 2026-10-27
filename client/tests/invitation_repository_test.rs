mod fixtures;

use uuid::Uuid;

use client::adapters::out::persistance::repositories::invitation::InvitationRepository;
use client::domain::dto::invitation::{Invitation, InvitationStatus};
use fixtures::{seed_chat, seed_user, setup_db};

async fn seeded_pair(db: &sea_orm::DatabaseConnection) -> (Uuid, Uuid, Uuid) {
    let chat_id = Uuid::new_v4();
    let inviter_id = Uuid::new_v4();
    let invitee_id = Uuid::new_v4();

    seed_user(db, inviter_id).await;
    seed_user(db, invitee_id).await;
    seed_chat(db, chat_id).await;

    (chat_id, inviter_id, invitee_id)
}

#[tokio::test]
async fn upsert_twice_with_the_same_invitation_keeps_a_single_row() {
    let db = setup_db().await;
    let (chat_id, inviter_id, invitee_id) = seeded_pair(&db).await;
    let repository = InvitationRepository::new(db.clone());

    let invitation = Invitation::outgoing(chat_id, inviter_id, invitee_id, None);

    repository.upsert(invitation.clone()).await.unwrap();
    repository.upsert(invitation.clone()).await.unwrap();

    let fetched = repository.get(invitation.id).await.unwrap();
    assert_eq!(fetched.id, invitation.id);
    assert_eq!(fetched.status, invitation.status);
    assert_eq!(fetched.direction, invitation.direction);

    let all = repository.list(None, None).await.unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn resolve_sets_status_and_responded_at() {
    let db = setup_db().await;
    let (chat_id, inviter_id, invitee_id) = seeded_pair(&db).await;
    let repository = InvitationRepository::new(db.clone());

    let invitation = Invitation::outgoing(chat_id, inviter_id, invitee_id, None);
    repository.upsert(invitation.clone()).await.unwrap();

    let resolved = repository
        .resolve(invitation.id, InvitationStatus::Accepted)
        .await
        .unwrap();

    assert_eq!(resolved.status, InvitationStatus::Accepted);
    assert!(resolved.responded_at.is_some());
}

#[tokio::test]
async fn find_pending_for_peer_only_finds_unresolved_invitations() {
    let db = setup_db().await;
    let (chat_id, inviter_id, invitee_id) = seeded_pair(&db).await;
    let repository = InvitationRepository::new(db.clone());

    let invitation = Invitation::outgoing(chat_id, inviter_id, invitee_id, None);
    repository.upsert(invitation.clone()).await.unwrap();

    let found = repository.find_pending_for_peer(invitee_id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, invitation.id);

    let found_by_inviter = repository.find_pending_for_peer(inviter_id).await.unwrap();
    assert!(found_by_inviter.is_some());

    repository
        .resolve(invitation.id, InvitationStatus::Rejected)
        .await
        .unwrap();

    let found_after_reject = repository.find_pending_for_peer(invitee_id).await.unwrap();
    assert!(found_after_reject.is_none());
}

#[tokio::test]
async fn resolve_lets_a_late_acceptance_overturn_a_cancellation() {
    let db = setup_db().await;
    let (chat_id, inviter_id, invitee_id) = seeded_pair(&db).await;
    let repository = InvitationRepository::new(db.clone());

    let invitation = Invitation::outgoing(chat_id, inviter_id, invitee_id, None);
    repository.upsert(invitation.clone()).await.unwrap();

    repository
        .resolve(invitation.id, InvitationStatus::Cancelled)
        .await
        .unwrap();

    let resolved = repository
        .resolve(invitation.id, InvitationStatus::Accepted)
        .await
        .unwrap();

    assert_eq!(resolved.status, InvitationStatus::Accepted);
}
