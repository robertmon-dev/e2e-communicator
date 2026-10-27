mod fixtures;

use std::collections::HashSet;

use uuid::Uuid;

use client::domain::dto::chat::ChatType;
use client::domain::dto::errors::{InviteError, PeerDirectoryError};
use client::domain::dto::invitation::{InvitationDirection, InvitationStatus};
use common::envelope::EventType;
use common::identity::{
    FINGERPRINT_LEN, encode_contact_code, signing_public_key, user_fingerprint,
};
use fixtures::{inviter_harness, setup_db};

fn contact_code_for(user_id: Uuid, user_seed: &[u8; common::identity::KEY_LEN]) -> String {
    let signing_key = signing_public_key(user_seed);
    encode_contact_code(user_id, &user_fingerprint(&signing_key))
}

#[tokio::test]
async fn invite_creates_an_inactive_direct_chat_and_one_envelope_per_peer_device() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let peer = harness.seed_peer_with_devices(2).await;

    let code = contact_code_for(peer.user_id, &peer.user_seed);
    let invitation = harness.inviter.invite(&code, "Alice", None).await.unwrap();

    assert_eq!(invitation.direction, InvitationDirection::Outgoing);
    assert_eq!(invitation.status, InvitationStatus::Pending);
    assert_eq!(invitation.inviter_user_id, harness.local_user_id);
    assert_eq!(invitation.invitee_user_id, peer.user_id);

    let chat = harness
        .chat_repository
        .get(invitation.chat_id)
        .await
        .unwrap();
    assert!(!chat.is_active);
    let latest = chat.get_latest().unwrap();
    assert_eq!(latest.chat_type, ChatType::Direct);

    let peer_row = harness.user_repository.get(peer.user_id).await.unwrap();
    assert_eq!(peer_row.nickname, "Alice");

    let due = harness.outbox_repository.list_due(10).await.unwrap();
    assert_eq!(due.len(), 2);

    let mut seen_devices = HashSet::new();
    for entry in &due {
        assert_eq!(entry.message_id, None);
        assert_eq!(entry.event_type, EventType::InvitationReceipt);
        assert_eq!(entry.recipient_id, peer.user_id);
        seen_devices.insert(entry.recipient_device_id);
    }

    let expected_devices: HashSet<Uuid> = peer.device_ids.iter().copied().collect();
    assert_eq!(seen_devices, expected_devices);
}

#[tokio::test]
async fn invite_with_a_mismatched_fingerprint_writes_nothing() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let peer = harness.seed_peer_with_devices(1).await;

    let wrong_fingerprint = [0u8; FINGERPRINT_LEN];
    let code = encode_contact_code(peer.user_id, &wrong_fingerprint);

    let result = harness.inviter.invite(&code, "Bob", None).await;

    assert!(matches!(
        result,
        Err(InviteError::PeerDirectory(PeerDirectoryError::IdentityMismatch(id))) if id == peer.user_id
    ));

    let due = harness.outbox_repository.list_due(10).await.unwrap();
    assert!(due.is_empty());

    let invitations = harness
        .invitation_repository
        .list(None, None)
        .await
        .unwrap();
    assert!(invitations.is_empty());
}

#[tokio::test]
async fn inviting_the_same_peer_twice_is_rejected() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let peer = harness.seed_peer_with_devices(1).await;

    let code = contact_code_for(peer.user_id, &peer.user_seed);

    harness.inviter.invite(&code, "Carl", None).await.unwrap();
    let second = harness.inviter.invite(&code, "Carl again", None).await;

    assert!(matches!(
        second,
        Err(InviteError::AlreadyInvited(id)) if id == peer.user_id
    ));
}

#[tokio::test]
async fn invite_rejects_a_malformed_contact_code() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;

    let result = harness
        .inviter
        .invite("not-a-valid-contact-code!!", "Dana", None)
        .await;

    assert!(matches!(result, Err(InviteError::InvalidContactCode)));
}

#[tokio::test]
async fn sealed_envelopes_are_non_empty_and_distinct_per_peer_device() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let peer = harness.seed_peer_with_devices(2).await;

    let code = contact_code_for(peer.user_id, &peer.user_seed);
    harness.inviter.invite(&code, "Eve", None).await.unwrap();

    let due = harness.outbox_repository.list_due(10).await.unwrap();
    assert_eq!(due.len(), 2);

    assert!(!due[0].ciphertext.is_empty());
    assert!(!due[1].ciphertext.is_empty());
    assert_ne!(due[0].ciphertext, due[1].ciphertext);
}

#[tokio::test]
async fn invite_reaches_a_brand_new_contact_and_keeps_the_pinned_identity() {
    let db = setup_db().await;
    let harness = fixtures::inviter_harness(&db).await;
    let peer = harness.seed_peer_devices_only(1);
    let code = encode_contact_code(
        peer.user_id,
        &user_fingerprint(&signing_public_key(&peer.user_seed)),
    );

    harness
        .inviter
        .invite(&code, "Bartek", None)
        .await
        .expect("pierwsze zaproszenie do nieznanego kontaktu musi przejsc");

    assert_eq!(
        harness
            .user_repository
            .get(peer.user_id)
            .await
            .unwrap()
            .nickname,
        "Bartek"
    );
    assert_eq!(
        harness
            .peer_devices
            .pinned_identity(peer.user_id)
            .await
            .unwrap(),
        Some(signing_public_key(&peer.user_seed).to_vec()),
        "zaproszenie nie moze wyciarac tozsamosci przypietej przez PeerDirectory"
    );
}

#[tokio::test]
async fn accept_activates_the_chat_and_answers_with_our_own_key() {
    let db = setup_db().await;
    let harness = fixtures::inviter_harness(&db).await;
    let (invitation, _peer) = harness.seed_incoming_invitation().await;
    harness.outbox_repository.clear().await.unwrap();

    harness.inviter.accept(invitation.id).await.unwrap();

    let chat = harness
        .chat_repository
        .get(invitation.chat_id)
        .await
        .unwrap();
    assert!(chat.is_active, "akceptacja musi aktywowac czat");

    let stored = harness
        .invitation_repository
        .get(invitation.id)
        .await
        .unwrap();
    assert_eq!(stored.status, InvitationStatus::Accepted);
    assert!(stored.responded_at.is_some());

    let due = harness.outbox_repository.list_due(10).await.unwrap();
    assert!(!due.is_empty(), "odpowiedz musi trafic do outboksu");
    assert!(
        due.iter()
            .all(|entry| entry.event_type == EventType::InvitationResponse)
    );

    let own = harness
        .key_repository
        .list_by_chat_and_user(invitation.chat_id, harness.local_user_id)
        .await
        .unwrap();
    assert_eq!(own.len(), 1);
    assert!(
        own[0].encrypted_private_key.is_some(),
        "wlasny klucz czatu musi miec czesc prywatna"
    );
}

#[tokio::test]
async fn accept_works_without_touching_the_network() {
    let db = setup_db().await;
    let harness = fixtures::inviter_harness(&db).await;
    let (invitation, _peer) = harness.seed_incoming_invitation().await;
    harness.outbox_repository.clear().await.unwrap();
    let lookups_before = harness.transport.count("lookup_devices");

    harness.inviter.accept(invitation.id).await.unwrap();

    assert_eq!(
        harness.transport.count("lookup_devices"),
        lookups_before,
        "akceptacja musi dzialac offline, z przypietych urzadzen"
    );
}

#[tokio::test]
async fn reject_notifies_the_inviter_without_activating_the_chat() {
    let db = setup_db().await;
    let harness = fixtures::inviter_harness(&db).await;
    let (invitation, _peer) = harness.seed_incoming_invitation().await;
    harness.outbox_repository.clear().await.unwrap();

    harness.inviter.reject(invitation.id).await.unwrap();

    let stored = harness
        .invitation_repository
        .get(invitation.id)
        .await
        .unwrap();
    assert_eq!(stored.status, InvitationStatus::Rejected);
    assert!(
        !harness
            .chat_repository
            .get(invitation.chat_id)
            .await
            .unwrap()
            .is_active,
        "odmowa nie moze aktywowac czatu"
    );
    assert!(
        !harness
            .outbox_repository
            .list_due(10)
            .await
            .unwrap()
            .is_empty(),
        "zapraszajacy musi dowiedziec sie o odmowie"
    );
}

#[tokio::test]
async fn resolving_an_invitation_twice_is_a_wrong_state() {
    let db = setup_db().await;
    let harness = fixtures::inviter_harness(&db).await;
    let (invitation, _peer) = harness.seed_incoming_invitation().await;

    harness.inviter.accept(invitation.id).await.unwrap();
    let again = harness.inviter.accept(invitation.id).await;

    assert!(matches!(again, Err(InviteError::WrongState(id)) if id == invitation.id));
}

#[tokio::test]
async fn accept_on_an_unknown_invitation_is_not_found() {
    let db = setup_db().await;
    let harness = fixtures::inviter_harness(&db).await;
    let missing = Uuid::new_v4();

    let result = harness.inviter.accept(missing).await;

    assert!(matches!(result, Err(InviteError::NotFound(id)) if id == missing));
}

#[tokio::test]
async fn cancel_changes_only_local_state_and_sends_nothing() {
    let db = setup_db().await;
    let harness = fixtures::inviter_harness(&db).await;
    let peer = harness.seed_peer_devices_only(1);
    let code = encode_contact_code(
        peer.user_id,
        &user_fingerprint(&signing_public_key(&peer.user_seed)),
    );
    let invitation = harness.inviter.invite(&code, "Bartek", None).await.unwrap();
    harness.outbox_repository.clear().await.unwrap();

    harness.inviter.cancel(invitation.id).await.unwrap();

    assert_eq!(
        harness
            .invitation_repository
            .get(invitation.id)
            .await
            .unwrap()
            .status,
        InvitationStatus::Cancelled
    );
    assert!(
        harness
            .outbox_repository
            .list_due(10)
            .await
            .unwrap()
            .is_empty(),
        "anulowania nie da sie wyslac - koperta juz lezy u peera"
    );
}
