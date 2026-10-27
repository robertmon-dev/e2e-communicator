mod fixtures;

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use orion::aead::SecretKey;
use orion::hazardous::ecc::x25519::{PrivateKey as X25519PrivateKey, PublicKey as X25519PublicKey};
use uuid::Uuid;

use client::adapters::out::keys::{DeviceKeyStore, DeviceKeyStoreRepository};
use client::adapters::out::persistance::repositories::chat::ChatRepository;
use client::adapters::out::persistance::repositories::chat::create::Create as ChatCreate;
use client::adapters::out::persistance::repositories::invitation::InvitationRepository;
use client::adapters::out::persistance::repositories::key::KeyRepository;
use client::adapters::out::persistance::repositories::message::MessageRepository;
use client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use client::adapters::out::persistance::repositories::peer_device::PeerDeviceRepository;
use client::adapters::out::persistance::repositories::user::UserRepository;
use client::domain::dto::chat::{Chat, ChatType};
use client::domain::dto::chat_key::ChatKeyBundle;
use client::domain::dto::invitation::{Invitation, InvitationDirection, InvitationStatus};
use client::domain::dto::local_identity::LocalIdentity;
use client::domain::dto::permission::MemberRole;
use client::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};
use client::domain::services::exchanger::Exchanger;
use client::domain::services::inbox::Inbox;
use client::domain::services::inviter::{
    InvitationPayload, InvitationResponsePayload, Inviter, MemberAddedPayload,
};
use client::domain::services::peer_directory::PeerDirectory;
use client::domain::state::connectivity::ConnectivityState;
use client::domain::state::facade_events::{FacadeEvent, FacadeEventBus, facade_event_bus};
use client::domain::state::identity::{IdentityState, IdentityStatus};
use client::domain::state::session::{SessionState, SessionToken};
use common::envelope::{EncryptedEnvelope, EventType};
use common::identity::{DevicePublicKeys, KEY_LEN, canonical_chat_key_bytes, random_bytes};
use fixtures::{FakeIdentityTransport, NoopSubscription, device_key_store, seed_user, setup_db};

struct LocalHarness {
    db: Arc<sea_orm::DatabaseConnection>,
    local_user_id: Uuid,
    local_device_id: Uuid,
    local_keys: DevicePublicKeys,
    transport: Arc<FakeIdentityTransport>,
    invitation_repository: Arc<InvitationRepository>,
    chat_repository: Arc<ChatRepository>,
    user_repository: Arc<UserRepository>,
    key_repository: Arc<KeyRepository>,
    outbox_repository: Arc<OutboxRepository>,
    events: FacadeEventBus,
    inbox: Inbox<NoopSubscription>,
}

async fn local_harness() -> LocalHarness {
    let db = setup_db().await;
    let local_user_id = Uuid::new_v4();
    let local_device_id = Uuid::new_v4();
    seed_user(&db, local_user_id).await;

    let device_keys = device_key_store(&db);
    let local_keys = device_keys
        .get_or_create_keys(local_device_id)
        .await
        .unwrap();

    let local_identity =
        LocalIdentity::new(local_user_id, local_device_id, &local_keys, vec![9u8; 64]);
    let identity_state = Arc::new(IdentityState::new(IdentityStatus::Registered(
        local_identity,
    )));

    let session_state = Arc::new(SessionState::default());
    session_state.set(SessionToken {
        token: "test-session".to_string(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    });

    let transport = FakeIdentityTransport::new();
    let peer_devices = Arc::new(PeerDeviceRepository::new(db.clone()));
    let peer_directory = Arc::new(PeerDirectory::new(
        transport.clone(),
        peer_devices,
        identity_state.clone(),
    ));

    let key_repository = Arc::new(KeyRepository::new(db.clone()));
    let exchanger = Arc::new(Exchanger::new(key_repository.clone(), SecretKey::default()));
    let chat_repository = Arc::new(ChatRepository::new(db.clone()));
    let user_repository = Arc::new(UserRepository::new(db.clone()));
    let invitation_repository = Arc::new(InvitationRepository::new(db.clone()));
    let outbox_repository = Arc::new(OutboxRepository::new(db.clone()));
    let message_repository = Arc::new(MessageRepository::new(db.clone()));

    let events = facade_event_bus();

    let inviter = Arc::new(Inviter::new(
        peer_directory.clone(),
        exchanger.clone(),
        device_keys.clone(),
        chat_repository.clone(),
        user_repository.clone(),
        invitation_repository.clone(),
        outbox_repository.clone(),
        key_repository.clone(),
        identity_state.clone(),
    ));

    let inbox = Inbox::new(
        Arc::new(NoopSubscription),
        message_repository,
        outbox_repository.clone(),
        exchanger,
        Arc::new(ConnectivityState::default()),
        identity_state.clone(),
        session_state,
        events.clone(),
        peer_directory,
        invitation_repository.clone(),
        chat_repository.clone(),
        user_repository.clone(),
        device_keys,
        key_repository.clone(),
        inviter,
    );

    LocalHarness {
        db,
        local_user_id,
        local_device_id,
        local_keys,
        transport,
        invitation_repository,
        chat_repository,
        user_repository,
        key_repository,
        outbox_repository,
        events,
        inbox,
    }
}

struct PeerHarness {
    user_id: Uuid,
    user_seed: [u8; KEY_LEN],
    device_id: Uuid,
    device_keys: Arc<DeviceKeyStore>,
}

async fn peer_harness(transport: &Arc<FakeIdentityTransport>) -> PeerHarness {
    let peer_db = setup_db().await;
    let user_id = Uuid::new_v4();
    let user_seed = random_bytes::<KEY_LEN>().unwrap();
    let device_id = Uuid::new_v4();

    let device_keys = device_key_store(&peer_db);
    let keys = device_keys.get_or_create_keys(device_id).await.unwrap();

    transport.seed_device_with_keys(user_id, &user_seed, device_id, &keys);

    PeerHarness {
        user_id,
        user_seed,
        device_id,
        device_keys,
    }
}

async fn add_peer_device(peer: &PeerHarness, transport: &Arc<FakeIdentityTransport>) -> Uuid {
    let device_id = Uuid::new_v4();
    let keys = peer
        .device_keys
        .get_or_create_keys(device_id)
        .await
        .unwrap();
    transport.seed_device_with_keys(peer.user_id, &peer.user_seed, device_id, &keys);
    device_id
}

async fn signed_bundle(
    device_keys: &Arc<DeviceKeyStore>,
    bundle_chat_id: Uuid,
    bundle_user_id: Uuid,
    bundle_device_id: Uuid,
    signing_device_id: Uuid,
) -> ChatKeyBundle {
    let chat_private = X25519PrivateKey::generate();
    let chat_public = X25519PublicKey::try_from(&chat_private).unwrap();
    let chat_public_key: [u8; KEY_LEN] = chat_public.to_bytes();

    let canonical = canonical_chat_key_bytes(
        bundle_chat_id,
        bundle_user_id,
        bundle_device_id,
        &chat_public_key,
    );
    let signature = device_keys
        .sign(signing_device_id, &canonical)
        .await
        .unwrap();

    ChatKeyBundle {
        chat_id: bundle_chat_id,
        user_id: bundle_user_id,
        device_id: bundle_device_id,
        chat_public_key,
        signature,
    }
}

#[allow(clippy::too_many_arguments)]
async fn seal_from_peer(
    peer_device_keys: &Arc<DeviceKeyStore>,
    peer_user_id: Uuid,
    peer_device_id: Uuid,
    local: &LocalHarness,
    chat_id: Uuid,
    event_type: EventType,
    payload: &impl serde::Serialize,
) -> EncryptedEnvelope {
    let shared_key = peer_device_keys
        .derive_device_shared_key(
            peer_device_id,
            local.local_device_id,
            &local.local_keys.agreement_key,
        )
        .await
        .unwrap();

    let plaintext = serde_json::to_vec(payload).unwrap();
    let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext).unwrap();

    EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id: peer_user_id,
        sender_device_id: peer_device_id,
        recipient_id: local.local_user_id,
        recipient_device_id: Some(local.local_device_id),
        created_at: Utc::now(),
        event_type,
        notify: true,
        ciphertext,
    }
}

#[tokio::test]
async fn receiving_an_invitation_creates_an_inactive_chat_and_a_pending_invitation() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;

    let chat_id = Uuid::new_v4();
    let invitation_id = Uuid::new_v4();

    let bundle = signed_bundle(
        &peer.device_keys,
        chat_id,
        peer.user_id,
        peer.device_id,
        peer.device_id,
    )
    .await;

    let payload = InvitationPayload {
        invitation_id,
        chat_id,
        chat_type: ChatType::Direct,
        chat_name: None,
        inviter_nickname: "Alice".to_string(),
        members: Vec::new(),
        key_bundle: bundle,
    };

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::InvitationReceipt,
        &payload,
    )
    .await;

    let mut event_receiver = local.events.subscribe();

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let chat = local.chat_repository.get(chat_id).await.unwrap();
    assert!(
        !chat.is_active,
        "an incoming invitation must not activate the chat yet"
    );

    let invitation = local
        .invitation_repository
        .find_by_chat(chat_id)
        .await
        .unwrap()
        .expect("invitation must be persisted");
    assert_eq!(invitation.id, invitation_id);
    assert_eq!(invitation.direction, InvitationDirection::Incoming);
    assert_eq!(invitation.status, InvitationStatus::Pending);
    assert_eq!(invitation.inviter_user_id, peer.user_id);
    assert_eq!(invitation.invitee_user_id, local.local_user_id);

    let peer_keys = local
        .key_repository
        .list_by_chat_and_user(chat_id, peer.user_id)
        .await
        .unwrap();
    assert_eq!(peer_keys.len(), 1);
    assert!(
        peer_keys[0].encrypted_private_key.is_none(),
        "a peer's own key row must never carry a private part"
    );

    let peer_row = local.user_repository.get(peer.user_id).await.unwrap();
    assert_eq!(peer_row.nickname, "Alice");

    let event = event_receiver.try_recv().unwrap();
    match event {
        FacadeEvent::InvitationReceived(received) => assert_eq!(received.id, invitation_id),
        other => panic!("expected InvitationReceived, got {other:?}"),
    }
}

#[tokio::test]
async fn a_key_bundle_signed_by_another_device_is_rejected_without_any_state_change() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;
    let other_device_id = add_peer_device(&peer, &local.transport).await;

    let chat_id = Uuid::new_v4();
    let invitation_id = Uuid::new_v4();

    let bundle = signed_bundle(
        &peer.device_keys,
        chat_id,
        peer.user_id,
        peer.device_id,
        other_device_id,
    )
    .await;

    let payload = InvitationPayload {
        invitation_id,
        chat_id,
        chat_type: ChatType::Direct,
        chat_name: None,
        inviter_nickname: "Mallory".to_string(),
        members: Vec::new(),
        key_bundle: bundle,
    };

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::InvitationReceipt,
        &payload,
    )
    .await;

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    assert!(
        local
            .invitation_repository
            .find_by_chat(chat_id)
            .await
            .unwrap()
            .is_none(),
        "a broken trust chain must not leave an invitation behind"
    );
    assert!(
        local.chat_repository.get(chat_id).await.is_err(),
        "a broken trust chain must not leave a chat behind"
    );
}

#[tokio::test]
async fn a_key_bundle_for_a_different_chat_is_rejected_without_any_state_change() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;

    let chat_id = Uuid::new_v4();
    let other_chat_id = Uuid::new_v4();
    let invitation_id = Uuid::new_v4();

    let bundle = signed_bundle(
        &peer.device_keys,
        other_chat_id,
        peer.user_id,
        peer.device_id,
        peer.device_id,
    )
    .await;

    let payload = InvitationPayload {
        invitation_id,
        chat_id,
        chat_type: ChatType::Direct,
        chat_name: None,
        inviter_nickname: "Mallory".to_string(),
        members: Vec::new(),
        key_bundle: bundle,
    };

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::InvitationReceipt,
        &payload,
    )
    .await;

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    assert!(
        local
            .invitation_repository
            .find_by_chat(chat_id)
            .await
            .unwrap()
            .is_none(),
        "a mismatched key bundle must not leave an invitation behind"
    );
    assert!(
        local.chat_repository.get(chat_id).await.is_err(),
        "a mismatched key bundle must not leave a chat behind"
    );
}

#[tokio::test]
async fn redelivering_the_same_invitation_envelope_is_idempotent() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;

    let chat_id = Uuid::new_v4();
    let invitation_id = Uuid::new_v4();

    let bundle = signed_bundle(
        &peer.device_keys,
        chat_id,
        peer.user_id,
        peer.device_id,
        peer.device_id,
    )
    .await;

    let payload = InvitationPayload {
        invitation_id,
        chat_id,
        chat_type: ChatType::Direct,
        chat_name: None,
        inviter_nickname: "Alice".to_string(),
        members: Vec::new(),
        key_bundle: bundle,
    };

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::InvitationReceipt,
        &payload,
    )
    .await;

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope.clone())
        .await;
    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let invitations = local.invitation_repository.list(None, None).await.unwrap();
    assert_eq!(
        invitations.len(),
        1,
        "redelivery must not duplicate the invitation"
    );

    let peer_keys = local
        .key_repository
        .list_by_chat_and_user(chat_id, peer.user_id)
        .await
        .unwrap();
    assert_eq!(
        peer_keys.len(),
        1,
        "redelivery must not duplicate the peer's key"
    );
}

#[tokio::test]
async fn an_accepted_response_stores_the_peer_key_and_activates_the_chat() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;
    seed_user(&local.db, peer.user_id).await;

    let chat_id = Uuid::new_v4();

    let mut members = BTreeMap::new();
    members.insert(local.local_user_id, MemberRole::Admin);
    members.insert(peer.user_id, MemberRole::Admin);
    local
        .chat_repository
        .create(Chat::with_id(
            chat_id,
            ChatType::Direct,
            None,
            members,
            false,
        ))
        .await
        .unwrap();

    let invitation = Invitation::outgoing(chat_id, local.local_user_id, peer.user_id, None);
    local
        .invitation_repository
        .upsert(invitation.clone())
        .await
        .unwrap();

    let bundle = signed_bundle(
        &peer.device_keys,
        chat_id,
        peer.user_id,
        peer.device_id,
        peer.device_id,
    )
    .await;

    let payload = InvitationResponsePayload {
        invitation_id: invitation.id,
        chat_id,
        accepted: true,
        key_bundle: Some(bundle),
    };

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::InvitationResponse,
        &payload,
    )
    .await;

    let mut event_receiver = local.events.subscribe();

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let chat = local.chat_repository.get(chat_id).await.unwrap();
    assert!(
        chat.is_active,
        "an accepted response must activate the chat"
    );

    let stored = local
        .invitation_repository
        .get(invitation.id)
        .await
        .unwrap();
    assert_eq!(stored.status, InvitationStatus::Accepted);

    let peer_keys = local
        .key_repository
        .list_by_chat_and_user(chat_id, peer.user_id)
        .await
        .unwrap();
    assert_eq!(peer_keys.len(), 1);

    let mut saw_activated = false;
    let mut saw_resolved = false;
    while let Ok(event) = event_receiver.try_recv() {
        match event {
            FacadeEvent::ChatActivated(id) if id == chat_id => saw_activated = true,
            FacadeEvent::InvitationResolved {
                invitation_id: id,
                status,
            } if id == invitation.id && status == InvitationStatus::Accepted => saw_resolved = true,
            _ => {}
        }
    }
    assert!(saw_activated, "expected a ChatActivated event");
    assert!(
        saw_resolved,
        "expected an InvitationResolved(Accepted) event"
    );
}

#[tokio::test]
async fn a_rejected_response_resolves_the_invitation_without_activating_the_chat() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;
    seed_user(&local.db, peer.user_id).await;

    let chat_id = Uuid::new_v4();

    let mut members = BTreeMap::new();
    members.insert(local.local_user_id, MemberRole::Admin);
    members.insert(peer.user_id, MemberRole::Admin);
    local
        .chat_repository
        .create(Chat::with_id(
            chat_id,
            ChatType::Direct,
            None,
            members,
            false,
        ))
        .await
        .unwrap();

    let invitation = Invitation::outgoing(chat_id, local.local_user_id, peer.user_id, None);
    local
        .invitation_repository
        .upsert(invitation.clone())
        .await
        .unwrap();

    let payload = InvitationResponsePayload {
        invitation_id: invitation.id,
        chat_id,
        accepted: false,
        key_bundle: None,
    };

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::InvitationResponse,
        &payload,
    )
    .await;

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let stored = local
        .invitation_repository
        .get(invitation.id)
        .await
        .unwrap();
    assert_eq!(stored.status, InvitationStatus::Rejected);

    let chat = local.chat_repository.get(chat_id).await.unwrap();
    assert!(!chat.is_active, "a rejection must not activate the chat");
}

#[tokio::test]
async fn a_late_acceptance_wins_over_a_prior_cancellation() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;
    seed_user(&local.db, peer.user_id).await;

    let chat_id = Uuid::new_v4();

    let mut members = BTreeMap::new();
    members.insert(local.local_user_id, MemberRole::Admin);
    members.insert(peer.user_id, MemberRole::Admin);
    local
        .chat_repository
        .create(Chat::with_id(
            chat_id,
            ChatType::Direct,
            None,
            members,
            false,
        ))
        .await
        .unwrap();

    let invitation = Invitation::outgoing(chat_id, local.local_user_id, peer.user_id, None);
    local
        .invitation_repository
        .upsert(invitation.clone())
        .await
        .unwrap();

    local
        .invitation_repository
        .resolve(invitation.id, InvitationStatus::Cancelled)
        .await
        .unwrap();

    let bundle = signed_bundle(
        &peer.device_keys,
        chat_id,
        peer.user_id,
        peer.device_id,
        peer.device_id,
    )
    .await;

    let payload = InvitationResponsePayload {
        invitation_id: invitation.id,
        chat_id,
        accepted: true,
        key_bundle: Some(bundle),
    };

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::InvitationResponse,
        &payload,
    )
    .await;

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let chat = local.chat_repository.get(chat_id).await.unwrap();
    assert!(
        chat.is_active,
        "a late acceptance must still activate the chat"
    );

    let stored = local
        .invitation_repository
        .get(invitation.id)
        .await
        .unwrap();
    assert_eq!(
        stored.status,
        InvitationStatus::Accepted,
        "acceptance must win over a prior cancellation"
    );
}

#[tokio::test]
async fn member_added_from_a_manage_members_sender_adds_the_member_and_announces_the_own_key() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;
    seed_user(&local.db, peer.user_id).await;

    let chat_id = Uuid::new_v4();
    let mut members = BTreeMap::new();
    members.insert(local.local_user_id, MemberRole::Member);
    members.insert(peer.user_id, MemberRole::Admin);
    local
        .chat_repository
        .create(Chat::with_id(
            chat_id,
            ChatType::Group,
            Some("Squad".to_string()),
            members,
            true,
        ))
        .await
        .unwrap();

    let new_member_id = Uuid::new_v4();
    let new_member_seed = random_bytes::<KEY_LEN>().unwrap();
    local
        .transport
        .seed_certified_device(new_member_id, &new_member_seed);

    let payload = MemberAddedPayload {
        chat_id,
        user_id: new_member_id,
        nickname: "Nowy".to_string(),
    };

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::MemberAdded,
        &payload,
    )
    .await;

    let mut event_receiver = local.events.subscribe();

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let chat = local.chat_repository.get(chat_id).await.unwrap();
    let latest = chat.get_latest().unwrap();
    assert_eq!(
        latest.members.get(&new_member_id),
        Some(&MemberRole::Member)
    );

    let new_member_row = local.user_repository.get(new_member_id).await.unwrap();
    assert_eq!(new_member_row.nickname, "Nowy");

    let mut saw_member_added = false;
    while let Ok(event) = event_receiver.try_recv() {
        if let FacadeEvent::MemberAdded {
            chat_id: event_chat_id,
            user_id,
        } = event
        {
            assert_eq!(event_chat_id, chat_id);
            assert_eq!(user_id, new_member_id);
            saw_member_added = true;
        }
    }
    assert!(saw_member_added, "expected a MemberAdded event");

    let due = local.outbox_repository.list_due(10).await.unwrap();
    assert!(
        due.iter()
            .any(|entry| entry.event_type == EventType::ChatKeyAnnouncement
                && entry.recipient_id == new_member_id),
        "expected a ChatKeyAnnouncement sent to the new member"
    );
}

#[tokio::test]
async fn member_added_from_a_sender_without_manage_members_is_rejected() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;
    seed_user(&local.db, peer.user_id).await;

    let chat_id = Uuid::new_v4();
    let mut members = BTreeMap::new();
    members.insert(local.local_user_id, MemberRole::Admin);
    members.insert(peer.user_id, MemberRole::Member);
    local
        .chat_repository
        .create(Chat::with_id(
            chat_id,
            ChatType::Group,
            Some("Squad".to_string()),
            members,
            true,
        ))
        .await
        .unwrap();

    let new_member_id = Uuid::new_v4();

    let payload = MemberAddedPayload {
        chat_id,
        user_id: new_member_id,
        nickname: "Nowy".to_string(),
    };

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::MemberAdded,
        &payload,
    )
    .await;

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let chat = local.chat_repository.get(chat_id).await.unwrap();
    let latest = chat.get_latest().unwrap();
    assert!(
        !latest.members.contains_key(&new_member_id),
        "a member without MANAGE_MEMBERS must not be able to add anyone"
    );

    let due = local.outbox_repository.list_due(10).await.unwrap();
    assert!(
        due.is_empty(),
        "a rejected MemberAdded must not send anything"
    );
}

#[tokio::test]
async fn chat_key_announcement_stores_the_senders_key_without_granting_membership() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;

    let chat_id = Uuid::new_v4();
    let mut members = BTreeMap::new();
    members.insert(local.local_user_id, MemberRole::Admin);
    local
        .chat_repository
        .create(Chat::with_id(
            chat_id,
            ChatType::Group,
            Some("Squad".to_string()),
            members,
            true,
        ))
        .await
        .unwrap();

    let bundle = signed_bundle(
        &peer.device_keys,
        chat_id,
        peer.user_id,
        peer.device_id,
        peer.device_id,
    )
    .await;

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::ChatKeyAnnouncement,
        &bundle,
    )
    .await;

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let peer_keys = local
        .key_repository
        .list_by_chat_and_user(chat_id, peer.user_id)
        .await
        .unwrap();
    assert_eq!(
        peer_keys.len(),
        1,
        "the sender's key must be stored even without membership (race rule)"
    );

    let chat = local.chat_repository.get(chat_id).await.unwrap();
    let latest = chat.get_latest().unwrap();
    assert!(
        !latest.members.contains_key(&peer.user_id),
        "a key announcement must never grant membership by itself"
    );
}

#[tokio::test]
async fn chat_key_announcement_for_an_unknown_chat_is_dropped() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;

    let chat_id = Uuid::new_v4();

    let bundle = signed_bundle(
        &peer.device_keys,
        chat_id,
        peer.user_id,
        peer.device_id,
        peer.device_id,
    )
    .await;

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::ChatKeyAnnouncement,
        &bundle,
    )
    .await;

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let peer_keys = local
        .key_repository
        .list_by_chat_and_user(chat_id, peer.user_id)
        .await
        .unwrap();
    assert!(
        peer_keys.is_empty(),
        "a key announcement for a chat we don't have locally must be dropped"
    );
}

#[tokio::test]
async fn chat_key_announcement_signed_by_another_device_is_rejected() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;
    let other_device_id = add_peer_device(&peer, &local.transport).await;

    let chat_id = Uuid::new_v4();
    let mut members = BTreeMap::new();
    members.insert(local.local_user_id, MemberRole::Admin);
    local
        .chat_repository
        .create(Chat::with_id(
            chat_id,
            ChatType::Group,
            Some("Squad".to_string()),
            members,
            true,
        ))
        .await
        .unwrap();

    let bundle = signed_bundle(
        &peer.device_keys,
        chat_id,
        peer.user_id,
        peer.device_id,
        other_device_id,
    )
    .await;

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::ChatKeyAnnouncement,
        &bundle,
    )
    .await;

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let peer_keys = local
        .key_repository
        .list_by_chat_and_user(chat_id, peer.user_id)
        .await
        .unwrap();
    assert!(
        peer_keys.is_empty(),
        "a key bundle signed by another device must not be stored"
    );
}

#[tokio::test]
async fn key_exchange_progress_reports_known_and_total_for_the_rest_of_the_group() {
    let local = local_harness().await;
    let peer = peer_harness(&local.transport).await;
    seed_user(&local.db, peer.user_id).await;

    let other_member_id = Uuid::new_v4();
    seed_user(&local.db, other_member_id).await;

    let chat_id = Uuid::new_v4();
    let mut members = BTreeMap::new();
    members.insert(local.local_user_id, MemberRole::Admin);
    members.insert(peer.user_id, MemberRole::Member);
    members.insert(other_member_id, MemberRole::Member);
    local
        .chat_repository
        .create(Chat::with_id(
            chat_id,
            ChatType::Group,
            Some("Squad".to_string()),
            members,
            true,
        ))
        .await
        .unwrap();

    let bundle = signed_bundle(
        &peer.device_keys,
        chat_id,
        peer.user_id,
        peer.device_id,
        peer.device_id,
    )
    .await;

    let envelope = seal_from_peer(
        &peer.device_keys,
        peer.user_id,
        peer.device_id,
        &local,
        chat_id,
        EventType::ChatKeyAnnouncement,
        &bundle,
    )
    .await;

    let mut event_receiver = local.events.subscribe();

    local
        .inbox
        .handle(Uuid::new_v4().to_string(), envelope)
        .await;

    let mut progress = None;
    while let Ok(event) = event_receiver.try_recv() {
        if let FacadeEvent::KeyExchangeProgress {
            chat_id: event_chat_id,
            known,
            total,
        } = event
        {
            assert_eq!(event_chat_id, chat_id);
            progress = Some((known, total));
        }
    }

    assert_eq!(
        progress,
        Some((1, 2)),
        "exactly one of the two other members has a known key"
    );
}
