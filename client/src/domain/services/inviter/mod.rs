use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use sea_orm::DbErr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use common::envelope::{EncryptedEnvelope, EventType};
use common::identity::{KEY_LEN, canonical_chat_key_bytes, decode_contact_code, fixed};

use crate::adapters::out::keys::{DeviceKeyStore, DeviceKeyStoreRepository};
use crate::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::adapters::out::persistance::repositories::chat::create::Create as ChatCreate;
use crate::adapters::out::persistance::repositories::chat::update::Update as ChatUpdate;
use crate::adapters::out::persistance::repositories::chat::utils::UpdateChatInput;
use crate::adapters::out::persistance::repositories::invitation::InvitationRepository;
use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::outbox::create::Create as OutboxCreate;
use crate::adapters::out::persistance::repositories::user::UserRepository;
use crate::adapters::out::persistance::repositories::user::update::Update as UserUpdate;
use crate::adapters::out::persistance::repositories::user::utils::UserUpdateInput;

use crate::domain::dto::chat::{Chat, ChatType};
use crate::domain::dto::chat_key::ChatKeyBundle;
use crate::domain::dto::errors::{InviteError, PeerDirectoryError};
use crate::domain::dto::invitation::{Invitation, InvitationStatus};
use crate::domain::dto::key::Key;
use crate::domain::dto::local_identity::LocalIdentity;
use crate::domain::dto::outbox::OutboxEntry;
use crate::domain::dto::permission::{MemberRole, Permissions};
use crate::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};
use crate::domain::services::exchanger::Exchanger;
use crate::domain::services::peer_directory::{PeerDirectory, VerifiedDevice};
use crate::domain::state::identity::IdentityState;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InvitationPayload {
    pub invitation_id: Uuid,
    pub chat_id: Uuid,
    pub chat_type: ChatType,
    pub chat_name: Option<String>,
    pub inviter_nickname: String,
    pub members: Vec<(Uuid, String)>,
    pub key_bundle: ChatKeyBundle,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InvitationResponsePayload {
    pub invitation_id: Uuid,
    pub chat_id: Uuid,
    pub accepted: bool,
    pub key_bundle: Option<ChatKeyBundle>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemberAddedPayload {
    pub chat_id: Uuid,
    pub user_id: Uuid,
    pub nickname: String,
}

pub struct Inviter {
    peer_directory: Arc<PeerDirectory>,
    exchanger: Arc<Exchanger>,
    device_keys: Arc<DeviceKeyStore>,
    chat_repository: Arc<ChatRepository>,
    user_repository: Arc<UserRepository>,
    invitation_repository: Arc<InvitationRepository>,
    outbox_repository: Arc<OutboxRepository>,
    key_repository: Arc<KeyRepository>,
    identity: Arc<IdentityState>,
}

impl Inviter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        peer_directory: Arc<PeerDirectory>,
        exchanger: Arc<Exchanger>,
        device_keys: Arc<DeviceKeyStore>,
        chat_repository: Arc<ChatRepository>,
        user_repository: Arc<UserRepository>,
        invitation_repository: Arc<InvitationRepository>,
        outbox_repository: Arc<OutboxRepository>,
        key_repository: Arc<KeyRepository>,
        identity: Arc<IdentityState>,
    ) -> Self {
        Self {
            peer_directory,
            exchanger,
            device_keys,
            chat_repository,
            user_repository,
            invitation_repository,
            outbox_repository,
            key_repository,
            identity,
        }
    }

    fn require_local(&self) -> Result<LocalIdentity, InviteError> {
        self.identity
            .registered()
            .ok_or(InviteError::PeerDirectory(PeerDirectoryError::Unregistered))
    }

    async fn own_key_bundle(
        &self,
        chat_id: Uuid,
        local: &LocalIdentity,
    ) -> Result<ChatKeyBundle, InviteError> {
        let key = self
            .exchanger
            .generate_local_key(chat_id, local.user_id, local.device_id)
            .await?;

        let chat_public_key: [u8; KEY_LEN] = fixed(&key.public_key)
            .map_err(|_| InviteError::BrokenTrustChain(local.user_id, local.device_id))?;

        let canonical =
            canonical_chat_key_bytes(chat_id, local.user_id, local.device_id, &chat_public_key);
        let signature = self.device_keys.sign(local.device_id, &canonical).await?;

        Ok(ChatKeyBundle {
            chat_id,
            user_id: local.user_id,
            device_id: local.device_id,
            chat_public_key,
            signature,
        })
    }

    async fn existing_or_new_key_bundle(
        &self,
        chat_id: Uuid,
        local: &LocalIdentity,
    ) -> Result<ChatKeyBundle, InviteError> {
        match self
            .key_repository
            .get_by_chat_user_device(chat_id, local.user_id, local.device_id)
            .await
        {
            Ok(existing) => self.bundle_from(chat_id, local, &existing).await,
            Err(DbErr::RecordNotFound(_)) => self.own_key_bundle(chat_id, local).await,
            Err(err) => Err(InviteError::Database(err)),
        }
    }

    async fn bundle_from(
        &self,
        chat_id: Uuid,
        local: &LocalIdentity,
        key: &Key,
    ) -> Result<ChatKeyBundle, InviteError> {
        let chat_public_key: [u8; KEY_LEN] = fixed(&key.public_key)
            .map_err(|_| InviteError::BrokenTrustChain(local.user_id, local.device_id))?;

        let signature = self
            .device_keys
            .sign(
                local.device_id,
                &canonical_chat_key_bytes(
                    chat_id,
                    local.user_id,
                    local.device_id,
                    &chat_public_key,
                ),
            )
            .await?;

        Ok(ChatKeyBundle {
            chat_id,
            user_id: local.user_id,
            device_id: local.device_id,
            chat_public_key,
            signature,
        })
    }

    async fn existing_group(
        &self,
        chat_id: Uuid,
        local: &LocalIdentity,
        peer_id: Uuid,
    ) -> Result<(Chat, Vec<(Uuid, String)>), InviteError> {
        let chat = self.chat_repository.get(chat_id).await?;
        let state = chat.get_latest()?;

        let role = state
            .members
            .get(&local.user_id)
            .ok_or(InviteError::Forbidden(chat_id))?;

        if !role.can(Permissions::MANAGE_MEMBERS) {
            return Err(InviteError::Forbidden(chat_id));
        }

        if state.members.contains_key(&peer_id) {
            return Err(InviteError::AlreadyInvited(peer_id));
        }

        let mut members = Vec::new();
        for member_id in state.members.keys() {
            if *member_id == local.user_id {
                continue;
            }

            let member = self.user_repository.get(*member_id).await?;
            members.push((*member_id, member.nickname));
        }

        Ok((chat, members))
    }

    async fn seal_to_devices<P: Serialize>(
        &self,
        local: &LocalIdentity,
        peer_id: Uuid,
        devices: &[VerifiedDevice],
        chat_id: Uuid,
        event_type: EventType,
        payload: &P,
    ) -> Result<(), InviteError> {
        let plaintext = serde_json::to_vec(payload)?;
        let mut entries = Vec::with_capacity(devices.len());

        for device in devices {
            let shared_key = self
                .device_keys
                .derive_device_shared_key(
                    local.device_id,
                    device.device_id,
                    &device.keys.agreement_key,
                )
                .await?;

            let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext)?;

            let envelope = EncryptedEnvelope {
                id: Uuid::new_v4(),
                chat_id,
                sender_id: local.user_id,
                sender_device_id: local.device_id,
                recipient_id: peer_id,
                recipient_device_id: Some(device.device_id),
                created_at: Utc::now(),
                event_type: event_type.clone(),
                notify: true,
                ciphertext,
            };

            entries.push(OutboxEntry::pending(None, peer_id, &envelope, None));
        }

        self.outbox_repository.create_many(entries).await?;

        Ok(())
    }

    pub async fn invite(
        &self,
        code: &str,
        label: &str,
        chat_id: Option<Uuid>,
    ) -> Result<Invitation, InviteError> {
        let contact = decode_contact_code(code).map_err(|_| InviteError::InvalidContactCode)?;
        let local = self.require_local()?;

        if chat_id.is_none()
            && self
                .invitation_repository
                .find_pending_for_peer(contact.user_id)
                .await?
                .is_some()
        {
            return Err(InviteError::AlreadyInvited(contact.user_id));
        }

        let peer = self
            .peer_directory
            .resolve(contact.user_id, Some(contact.fingerprint))
            .await?;

        self.user_repository
            .update_user(UserUpdateInput {
                id: contact.user_id,
                name: None,
                surname: None,
                nickname: Some(label.to_string()),
                phone: None,
            })
            .await?;

        let (chat, members) = match chat_id {
            None => {
                let mut initial_members = BTreeMap::new();
                initial_members.insert(local.user_id, MemberRole::Admin);
                initial_members.insert(contact.user_id, MemberRole::Admin);

                let chat = Chat::new(
                    ChatType::Direct,
                    None,
                    initial_members,
                    BTreeMap::new(),
                    None,
                    false,
                );
                self.chat_repository.create(chat.clone()).await?;
                (chat, Vec::new())
            }
            Some(id) => self.existing_group(id, &local, contact.user_id).await?,
        };

        let key_bundle = self.existing_or_new_key_bundle(chat.id, &local).await?;

        let invitation = Invitation::outgoing(
            chat.id,
            local.user_id,
            contact.user_id,
            Some(contact.fingerprint),
        );
        self.invitation_repository
            .upsert(invitation.clone())
            .await?;

        let inviter = self.user_repository.get(local.user_id).await?;
        let latest = chat.get_latest()?;

        let payload = InvitationPayload {
            invitation_id: invitation.id,
            chat_id: chat.id,
            chat_type: latest.chat_type.clone(),
            chat_name: latest.name.clone(),
            inviter_nickname: inviter.nickname,
            members,
            key_bundle,
        };

        self.seal_to_devices(
            &local,
            contact.user_id,
            &peer.devices,
            chat.id,
            EventType::InvitationReceipt,
            &payload,
        )
        .await?;

        Ok(invitation)
    }

    async fn require_pending(&self, invitation_id: Uuid) -> Result<Invitation, InviteError> {
        let invitation = match self.invitation_repository.get(invitation_id).await {
            Ok(invitation) => invitation,
            Err(DbErr::RecordNotFound(_)) => {
                return Err(InviteError::NotFound(invitation_id));
            }
            Err(err) => return Err(InviteError::Database(err)),
        };

        if invitation.status != InvitationStatus::Pending {
            return Err(InviteError::WrongState(invitation_id));
        }

        Ok(invitation)
    }

    pub async fn accept(&self, invitation_id: Uuid) -> Result<Invitation, InviteError> {
        let invitation = self.require_pending(invitation_id).await?;
        let local = self.require_local()?;

        let key_bundle = self.own_key_bundle(invitation.chat_id, &local).await?;
        let devices = self
            .peer_directory
            .cached(invitation.inviter_user_id)
            .await?;

        self.chat_repository
            .update_chat(UpdateChatInput {
                id: invitation.chat_id,
                is_active: Some(true),
                created_at: None,
                updated_at: Some(Utc::now()),
            })
            .await?;

        self.invitation_repository
            .resolve(invitation.id, InvitationStatus::Accepted)
            .await?;

        let payload = InvitationResponsePayload {
            invitation_id: invitation.id,
            chat_id: invitation.chat_id,
            accepted: true,
            key_bundle: Some(key_bundle),
        };

        self.seal_to_devices(
            &local,
            invitation.inviter_user_id,
            &devices,
            invitation.chat_id,
            EventType::InvitationResponse,
            &payload,
        )
        .await?;

        Ok(self.invitation_repository.get(invitation.id).await?)
    }

    pub async fn reject(&self, invitation_id: Uuid) -> Result<Invitation, InviteError> {
        let invitation = self.require_pending(invitation_id).await?;
        let local = self.require_local()?;

        let devices = self
            .peer_directory
            .cached(invitation.inviter_user_id)
            .await?;

        self.invitation_repository
            .resolve(invitation.id, InvitationStatus::Rejected)
            .await?;

        let payload = InvitationResponsePayload {
            invitation_id: invitation.id,
            chat_id: invitation.chat_id,
            accepted: false,
            key_bundle: None,
        };

        self.seal_to_devices(
            &local,
            invitation.inviter_user_id,
            &devices,
            invitation.chat_id,
            EventType::InvitationResponse,
            &payload,
        )
        .await?;

        Ok(self.invitation_repository.get(invitation.id).await?)
    }

    pub async fn cancel(&self, invitation_id: Uuid) -> Result<Invitation, InviteError> {
        let invitation = self.require_pending(invitation_id).await?;
        self.invitation_repository
            .resolve(invitation.id, InvitationStatus::Cancelled)
            .await?;

        Ok(self.invitation_repository.get(invitation.id).await?)
    }

    pub async fn create_group(&self, name: &str) -> Result<Chat, InviteError> {
        let local = self.require_local()?;

        let mut members = BTreeMap::new();
        members.insert(local.user_id, MemberRole::Admin);

        let chat = Chat::new(
            ChatType::Group,
            Some(name.to_string()),
            members,
            BTreeMap::new(),
            None,
            true,
        );
        self.chat_repository.create(chat.clone()).await?;

        self.own_key_bundle(chat.id, &local).await?;

        Ok(chat)
    }

    pub async fn announce_member_added(
        &self,
        chat_id: Uuid,
        new_member_id: Uuid,
        new_member_nickname: String,
    ) -> Result<(), InviteError> {
        let local = self.require_local()?;
        let chat = self.chat_repository.get(chat_id).await?;
        let state = chat.get_latest()?;

        let payload = MemberAddedPayload {
            chat_id,
            user_id: new_member_id,
            nickname: new_member_nickname,
        };

        for member_id in state.members.keys() {
            if *member_id == local.user_id || *member_id == new_member_id {
                continue;
            }

            let devices = match self.peer_directory.cached(*member_id).await {
                Ok(devices) => devices,
                Err(PeerDirectoryError::NoVerifiedDevices(id)) => {
                    tracing::warn!(
                        member_id = %id,
                        "skipping MemberAdded announcement: member has no pinned devices"
                    );
                    continue;
                }
                Err(err) => return Err(InviteError::PeerDirectory(err)),
            };

            self.seal_to_devices(
                &local,
                *member_id,
                &devices,
                chat_id,
                EventType::MemberAdded,
                &payload,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn announce_own_key(
        &self,
        chat_id: Uuid,
        to_user_id: Uuid,
    ) -> Result<(), InviteError> {
        let local = self.require_local()?;

        let bundle = self.existing_or_new_key_bundle(chat_id, &local).await?;

        let devices = match self.peer_directory.cached(to_user_id).await {
            Ok(devices) => devices,
            Err(PeerDirectoryError::NoVerifiedDevices(_)) => {
                self.peer_directory.resolve(to_user_id, None).await?.devices
            }
            Err(err) => return Err(InviteError::PeerDirectory(err)),
        };

        self.seal_to_devices(
            &local,
            to_user_id,
            &devices,
            chat_id,
            EventType::ChatKeyAnnouncement,
            &bundle,
        )
        .await
    }
}
