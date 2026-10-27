mod fixtures;

use uuid::Uuid;

use client::domain::dto::chat::ChatType;
use client::domain::dto::chat_key::ChatKeyBundle;
use client::domain::dto::errors::InviteError;
use client::domain::dto::permission::MemberRole;
use client::domain::services::inviter::{InvitationPayload, MemberAddedPayload};
use common::envelope::EventType;
use common::identity::{
    KEY_LEN, encode_contact_code, random_bytes, signing_public_key, user_fingerprint,
};
use fixtures::{inviter_harness, setup_db};

fn contact_code_for(user_id: Uuid, user_seed: &[u8; KEY_LEN]) -> String {
    let signing_key = signing_public_key(user_seed);
    encode_contact_code(user_id, &user_fingerprint(&signing_key))
}

#[tokio::test]
async fn create_group_creates_an_active_group_chat_with_admin_and_one_own_key() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;

    let chat = harness.inviter.create_group("Squad").await.unwrap();

    assert!(chat.is_active, "a freshly created group must be active");
    let latest = chat.get_latest().unwrap();
    assert_eq!(latest.chat_type, ChatType::Group);
    assert_eq!(latest.name, Some("Squad".to_string()));
    assert_eq!(
        latest.members.get(&harness.local_user_id),
        Some(&MemberRole::Admin)
    );

    let own_keys = harness
        .key_repository
        .list_by_chat_and_user(chat.id, harness.local_user_id)
        .await
        .unwrap();
    assert_eq!(own_keys.len(), 1);
    assert!(own_keys[0].encrypted_private_key.is_some());
}

#[tokio::test]
async fn inviting_to_a_group_without_manage_members_permission_is_forbidden() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let chat = harness.seed_group_where_local_is_member().await;
    let peer = harness.seed_peer_with_devices(1).await;
    let code = contact_code_for(peer.user_id, &peer.user_seed);

    let result = harness.inviter.invite(&code, "Nowy", Some(chat.id)).await;

    assert!(matches!(result, Err(InviteError::Forbidden(id)) if id == chat.id));
}

#[tokio::test]
async fn inviting_someone_who_is_already_a_group_member_is_rejected() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let chat = harness.inviter.create_group("Squad").await.unwrap();
    let existing_member = harness.seed_member_with_devices(chat.id, "Ola", 1).await;

    let code = contact_code_for(existing_member.user_id, &existing_member.user_seed);
    let result = harness
        .inviter
        .invite(&code, "Ola again", Some(chat.id))
        .await;

    assert!(matches!(
        result,
        Err(InviteError::AlreadyInvited(id)) if id == existing_member.user_id
    ));
}

#[tokio::test]
async fn invite_to_a_group_carries_the_existing_member_list_to_the_new_invitee() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let chat = harness.inviter.create_group("Squad").await.unwrap();
    let existing_member_id = harness.seed_member_of(chat.id, "Ola").await;

    let new_peer = harness.seed_peer_with_devices(1).await;
    let code = contact_code_for(new_peer.user_id, &new_peer.user_seed);

    harness
        .inviter
        .invite(&code, "Nowy", Some(chat.id))
        .await
        .unwrap();

    let due = harness.outbox_repository.list_due(10).await.unwrap();
    assert!(!due.is_empty(), "the new invitee must receive an envelope");
    assert!(
        due.iter()
            .all(|entry| entry.event_type == EventType::InvitationReceipt)
    );
    assert!(
        due.iter()
            .all(|entry| entry.recipient_id == new_peer.user_id)
    );

    let payload: InvitationPayload = harness
        .last_payload_for(new_peer.user_id, EventType::InvitationReceipt)
        .await
        .expect("InvitationReceipt payload must unseal");

    assert_eq!(payload.chat_id, chat.id);
    assert_eq!(payload.chat_type, ChatType::Group);
    assert_eq!(
        payload.members,
        vec![(existing_member_id, "Ola".to_string())]
    );
}

#[tokio::test]
async fn announce_member_added_reaches_existing_members_but_not_the_new_one_or_self() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let chat = harness.inviter.create_group("Squad").await.unwrap();
    let existing = harness.seed_member_with_devices(chat.id, "Ola", 1).await;
    let new_member_id = harness.seed_member_of(chat.id, "Nowy").await;
    harness.outbox_repository.clear().await.unwrap();

    harness
        .inviter
        .announce_member_added(chat.id, new_member_id, "Nowy".to_string())
        .await
        .unwrap();

    let due = harness.outbox_repository.list_due(10).await.unwrap();
    assert!(!due.is_empty(), "the existing member must be notified");
    for entry in &due {
        assert_eq!(entry.event_type, EventType::MemberAdded);
        assert_eq!(entry.recipient_id, existing.user_id);
        assert_ne!(entry.recipient_id, new_member_id);
        assert_ne!(entry.recipient_id, harness.local_user_id);
    }

    let payload: MemberAddedPayload = harness
        .last_payload_for(existing.user_id, EventType::MemberAdded)
        .await
        .expect("MemberAdded payload must unseal");
    assert_eq!(payload.chat_id, chat.id);
    assert_eq!(payload.user_id, new_member_id);
    assert_eq!(payload.nickname, "Nowy");
}

#[tokio::test]
async fn announce_member_added_skips_a_member_without_pinned_devices() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let chat = harness.inviter.create_group("Squad").await.unwrap();
    let without_devices = harness.seed_member_of(chat.id, "BezUrzadzen").await;
    let with_devices = harness
        .seed_member_with_devices(chat.id, "ZUrzadzeniami", 1)
        .await;
    let new_member_id = harness.seed_member_of(chat.id, "Nowy").await;
    harness.outbox_repository.clear().await.unwrap();

    harness
        .inviter
        .announce_member_added(chat.id, new_member_id, "Nowy".to_string())
        .await
        .unwrap();

    let due = harness.outbox_repository.list_due(10).await.unwrap();
    assert!(
        !due.is_empty(),
        "the member with pinned devices must still be reached"
    );
    assert!(
        due.iter()
            .all(|entry| entry.recipient_id == with_devices.user_id)
    );
    assert!(
        due.iter()
            .all(|entry| entry.recipient_id != without_devices)
    );
}

#[tokio::test]
async fn announce_own_key_sends_without_minting_a_second_chat_key() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let chat = harness.inviter.create_group("Squad").await.unwrap();
    let new_member = harness.seed_member_with_devices(chat.id, "Nowy", 1).await;
    harness.outbox_repository.clear().await.unwrap();

    harness
        .inviter
        .announce_own_key(chat.id, new_member.user_id)
        .await
        .unwrap();
    harness
        .inviter
        .announce_own_key(chat.id, new_member.user_id)
        .await
        .unwrap();

    let own_keys = harness
        .key_repository
        .list_by_chat_and_user(chat.id, harness.local_user_id)
        .await
        .unwrap();
    assert_eq!(
        own_keys.len(),
        1,
        "two announcements must not mint a second chat key"
    );

    let due = harness.outbox_repository.list_due(10).await.unwrap();
    assert_eq!(due.len(), 2, "each announcement must send its own envelope");
    assert!(
        due.iter()
            .all(|entry| entry.event_type == EventType::ChatKeyAnnouncement)
    );
    assert!(
        due.iter()
            .all(|entry| entry.recipient_id == new_member.user_id)
    );

    let payload: ChatKeyBundle = harness
        .last_payload_for(new_member.user_id, EventType::ChatKeyAnnouncement)
        .await
        .expect("ChatKeyAnnouncement payload must unseal");
    assert_eq!(payload.chat_id, chat.id);
    assert_eq!(payload.user_id, harness.local_user_id);
    assert_eq!(payload.device_id, harness.local_device_id);
    assert_eq!(payload.chat_public_key.to_vec(), own_keys[0].public_key);
}

#[tokio::test]
async fn announce_own_key_resolves_devices_when_the_member_is_not_yet_pinned() {
    let db = setup_db().await;
    let harness = inviter_harness(&db).await;
    let chat = harness.inviter.create_group("Squad").await.unwrap();

    let new_member_id = harness.seed_member_of(chat.id, "Nowy").await;
    let user_seed = random_bytes::<KEY_LEN>().unwrap();
    harness
        .transport
        .seed_certified_device(new_member_id, &user_seed);
    harness.outbox_repository.clear().await.unwrap();

    harness
        .inviter
        .announce_own_key(chat.id, new_member_id)
        .await
        .unwrap();

    let due = harness.outbox_repository.list_due(10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].recipient_id, new_member_id);
    assert_eq!(due[0].event_type, EventType::ChatKeyAnnouncement);
}
