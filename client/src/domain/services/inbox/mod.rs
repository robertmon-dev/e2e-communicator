use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch::Receiver;

use chrono::Utc;
use tokio_stream::{Stream, StreamExt};
use uuid::Uuid;

use common::envelope::{EncryptedEnvelope, EventType};
use common::identity::KEY_LEN;

use crate::adapters::out::keys::{DeviceKeyStore, DeviceKeyStoreRepository};
use crate::adapters::out::persistance::repositories::chat::ChatRepository;
use crate::adapters::out::persistance::repositories::chat::create::Create as ChatCreate;
use crate::adapters::out::persistance::repositories::chat::update::Update as ChatUpdate;
use crate::adapters::out::persistance::repositories::chat::utils::UpdateChatInput;
use crate::adapters::out::persistance::repositories::invitation::InvitationRepository;
use crate::adapters::out::persistance::repositories::key::KeyRepository;
use crate::adapters::out::persistance::repositories::message::MessageRepository;
use crate::adapters::out::persistance::repositories::message::create::Create as MessageCreate;
use crate::adapters::out::persistance::repositories::message::remove::Remove;
use crate::adapters::out::persistance::repositories::outbox::OutboxRepository;
use crate::adapters::out::persistance::repositories::outbox::create::Create as OutboxCreate;
use crate::adapters::out::persistance::repositories::outbox::update::Update as OutboxUpdate;
use crate::adapters::out::persistance::repositories::user::UserRepository;
use crate::adapters::out::persistance::repositories::user::create::Create as UserCreate;
use crate::adapters::out::persistance::repositories::user::update::Update as UserUpdate;
use crate::adapters::out::persistance::repositories::user::utils::UserUpdateInput;
use crate::domain::dto::chat::{Chat, ChatType};
use crate::domain::dto::chat_key::ChatKeyBundle;
use crate::domain::dto::delivery_receipt::DeliveryReceipt;
use crate::domain::dto::errors::{InboxError, InviteError, TransportError};
use crate::domain::dto::invitation::{Invitation, InvitationStatus};
use crate::domain::dto::local_identity::LocalIdentity;
use crate::domain::dto::message::{Message, MessageDeletedWirePayload, MessageWirePayload};
use crate::domain::dto::outbox::OutboxEntry;
use crate::domain::dto::permission::{MemberRole, Permissions};
use crate::domain::dto::user::User;
use crate::domain::services::encryptor::{Encryptor, PrivateKeyEncryptor};
use crate::domain::services::exchanger::Exchanger;
use crate::domain::services::inviter::{
    InvitationPayload, InvitationResponsePayload, Inviter, MemberAddedPayload,
};
use crate::domain::services::peer_directory::PeerDirectory;
use crate::domain::state::connectivity::ConnectivityState;
use crate::domain::state::facade_events::{FacadeEvent, FacadeEventBus};
use crate::domain::state::identity::{IdentityState, IdentityStatus};
use crate::domain::state::session::SessionState;

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

pub type InboundStream =
    Pin<Box<dyn Stream<Item = Result<(String, EncryptedEnvelope), TransportError>> + Send>>;

pub trait EnvelopeSubscription {
    async fn subscribe(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<InboundStream, TransportError>;

    async fn ack(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        delivery_token: &str,
    ) -> Result<(), TransportError>;
}

pub struct Inbox<T: EnvelopeSubscription> {
    subscription: Arc<T>,
    message_repository: Arc<MessageRepository>,
    outbox_repository: Arc<OutboxRepository>,
    exchanger: Arc<Exchanger>,
    connectivity: Arc<ConnectivityState>,
    identity: Arc<IdentityState>,
    session: Arc<SessionState>,
    events: FacadeEventBus,
    peer_directory: Arc<PeerDirectory>,
    invitation_repository: Arc<InvitationRepository>,
    chat_repository: Arc<ChatRepository>,
    user_repository: Arc<UserRepository>,
    device_keys: Arc<DeviceKeyStore>,
    key_repository: Arc<KeyRepository>,
    inviter: Arc<Inviter>,
}

impl<T: EnvelopeSubscription> Inbox<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        subscription: Arc<T>,
        message_repository: Arc<MessageRepository>,
        outbox_repository: Arc<OutboxRepository>,
        exchanger: Arc<Exchanger>,
        connectivity: Arc<ConnectivityState>,
        identity: Arc<IdentityState>,
        session: Arc<SessionState>,
        events: FacadeEventBus,
        peer_directory: Arc<PeerDirectory>,
        invitation_repository: Arc<InvitationRepository>,
        chat_repository: Arc<ChatRepository>,
        user_repository: Arc<UserRepository>,
        device_keys: Arc<DeviceKeyStore>,
        key_repository: Arc<KeyRepository>,
        inviter: Arc<Inviter>,
    ) -> Self {
        Self {
            subscription,
            message_repository,
            outbox_repository,
            exchanger,
            connectivity,
            identity,
            session,
            events,
            peer_directory,
            invitation_repository,
            chat_repository,
            user_repository,
            device_keys,
            key_repository,
            inviter,
        }
    }

    pub async fn run(&self, _shutdown: Receiver<bool>) -> ! {
        loop {
            if !self.connectivity.is_online() {
                let mut receiver = self.connectivity.subscribe();
                while !self.connectivity.is_online() {
                    let _ = receiver.changed().await;
                }
                continue;
            }

            let Some(local) = self.identity.registered() else {
                let mut receiver = self.identity.subscribe();
                let _ = receiver
                    .wait_for(|status| matches!(status, IdentityStatus::Registered(_)))
                    .await;
                continue;
            };

            if self.session.get().is_none() {
                let mut receiver = self.session.subscribe();
                let _ = receiver.wait_for(|session| session.is_some()).await;
                continue;
            }

            match self
                .subscription
                .subscribe(local.user_id, local.device_id)
                .await
            {
                Ok(mut stream) => {
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok((delivery_token, envelope)) => {
                                self.handle(delivery_token, envelope).await;
                            }
                            Err(TransportError::Unauthenticated(reason)) => {
                                tracing::warn!(%reason, "Inbox stream rejected as unauthenticated");
                                self.session.clear();
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                }
                Err(TransportError::Unauthenticated(reason)) => {
                    tracing::warn!(%reason, "Inbox subscribe rejected as unauthenticated");
                    self.session.clear();
                }
                Err(_) => tokio::time::sleep(RECONNECT_DELAY).await,
            }
        }
    }

    pub async fn handle(&self, delivery_token: String, envelope: EncryptedEnvelope) {
        let Some(local) = self.identity.registered() else {
            tracing::warn!(
                envelope_id = %envelope.id,
                "Inbound envelope while unregistered, ignoring"
            );
            return;
        };

        if let Some(target) = envelope.recipient_device_id
            && target != local.device_id
        {
            tracing::warn!(
                envelope_id = %envelope.id,
                %target,
                "envelope addressed to another device, dropping"
            );
            return;
        }

        let _ = self
            .subscription
            .ack(local.user_id, local.device_id, &delivery_token)
            .await;

        let result = match envelope.event_type.clone() {
            EventType::Message => self.handle_message(&local, &envelope).await,
            EventType::DeliveryReceipt => self.handle_delivery_receipt(&local, &envelope).await,
            EventType::MessageDeleted => self.handle_message_deleted(&local, &envelope).await,
            EventType::InvitationReceipt => self.handle_invitation(&local, &envelope).await,
            EventType::InvitationResponse => {
                self.handle_invitation_response(&local, &envelope).await
            }
            EventType::MemberAdded => self.handle_member_added(&local, &envelope).await,
            EventType::ChatKeyAnnouncement => {
                self.handle_chat_key_announcement(&local, &envelope).await
            }
            EventType::Reaction | EventType::ReadReceipt | EventType::OverlayReceipt => Ok(()),
        };

        if let Err(e) = result {
            tracing::warn!("Failed to handle inbound envelope {}: {e}", envelope.id);
        }
    }

    async fn handle_message(
        &self,
        local: &LocalIdentity,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(
                envelope.chat_id,
                (local.user_id, local.device_id),
                (envelope.sender_id, envelope.sender_device_id),
            )
            .await?;

        let plaintext = Encryptor::decrypt_with_derived_key(&shared_key, &envelope.ciphertext)?;
        let payload: MessageWirePayload = serde_json::from_slice(&plaintext)?;

        let message_id = payload.message_id;
        let reply_to_id = payload.reply_to_id;
        let available_to = payload.available_to;

        let mut history = BTreeMap::new();
        history.insert(envelope.created_at, payload.content);

        let message = Message {
            id: message_id,
            author_id: envelope.sender_id,
            chat_id: envelope.chat_id,
            history,
            seen_by: BTreeMap::new(),
            created_at: envelope.created_at,
            updated_at: envelope.created_at,
            available_to,
            hidden: false,
            is_reply: reply_to_id.is_some(),
            reply_to_id,
        };

        self.message_repository.upsert(message.clone()).await?;
        let _ = self.events.send(FacadeEvent::MessageReceived(message));

        self.send_delivery_receipt(local, envelope, message_id)
            .await
    }

    async fn handle_message_deleted(
        &self,
        local: &LocalIdentity,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(
                envelope.chat_id,
                (local.user_id, local.device_id),
                (envelope.sender_id, envelope.sender_device_id),
            )
            .await?;

        let plaintext = Encryptor::decrypt_with_derived_key(&shared_key, &envelope.ciphertext)?;
        let payload: MessageDeletedWirePayload = serde_json::from_slice(&plaintext)?;

        self.message_repository
            .remove_message(payload.deleted_message_id)
            .await?;
        self.send_delivery_receipt(local, envelope, payload.deleted_message_id)
            .await
    }

    async fn unseal<P: serde::de::DeserializeOwned>(
        &self,
        local: &LocalIdentity,
        envelope: &EncryptedEnvelope,
    ) -> Result<(P, [u8; KEY_LEN]), InboxError> {
        let peer = self
            .peer_directory
            .resolve(envelope.sender_id, None)
            .await?;
        let device = peer
            .devices
            .iter()
            .find(|device| device.device_id == envelope.sender_device_id)
            .ok_or(InboxError::UnknownSenderDevice(envelope.sender_device_id))?;

        let sealing_key = self
            .device_keys
            .derive_device_shared_key(
                local.device_id,
                device.device_id,
                &device.keys.agreement_key,
            )
            .await?;

        let plaintext = Encryptor::decrypt_with_derived_key(&sealing_key, &envelope.ciphertext)?;

        Ok((serde_json::from_slice(&plaintext)?, device.keys.signing_key))
    }

    async fn handle_invitation(
        &self,
        local: &LocalIdentity,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let (payload, sender_signing_key): (InvitationPayload, [u8; KEY_LEN]) =
            self.unseal(local, envelope).await?;

        payload.key_bundle.verify(&sender_signing_key)?;

        if payload.key_bundle.chat_id != payload.chat_id
            || payload.key_bundle.user_id != envelope.sender_id
            || payload.key_bundle.device_id != envelope.sender_device_id
        {
            return Err(InboxError::MismatchedKeyBundle(payload.chat_id));
        }

        // nie upsert(User::contact(..)): to nadpisaloby signing_public_key
        // wartoscia None i wymazalo tozsamosc, ktora PeerDirectory wlasnie
        // przypiela w unseal() wyzej (patrz identyczny komentarz w Inviter::invite)
        self.user_repository
            .update_user(UserUpdateInput {
                id: envelope.sender_id,
                name: None,
                surname: None,
                nickname: Some(payload.inviter_nickname.clone()),
                phone: None,
            })
            .await?;

        if self.chat_repository.get(payload.chat_id).await.is_err() {
            let mut members = BTreeMap::new();
            members.insert(envelope.sender_id, MemberRole::Admin);
            members.insert(local.user_id, MemberRole::Admin);

            for (member_id, member_nickname) in &payload.members {
                members.insert(*member_id, MemberRole::Member);
                self.user_repository
                    .upsert(User::contact(*member_id, member_nickname.clone()))
                    .await?;
            }

            let chat = Chat::with_id(
                payload.chat_id,
                payload.chat_type.clone(),
                payload.chat_name.clone(),
                members,
                false,
            );
            self.chat_repository.create(chat).await?;
        }

        self.exchanger
            .store_peer_key(
                payload.chat_id,
                envelope.sender_id,
                envelope.sender_device_id,
                payload.key_bundle.chat_public_key.to_vec(),
            )
            .await?;

        let invitation = Invitation::incoming(
            payload.invitation_id,
            payload.chat_id,
            envelope.sender_id,
            local.user_id,
        );
        self.invitation_repository
            .upsert(invitation.clone())
            .await?;

        let _ = self
            .events
            .send(FacadeEvent::InvitationReceived(invitation));

        Ok(())
    }

    async fn handle_invitation_response(
        &self,
        local: &LocalIdentity,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let (payload, sender_signing_key): (InvitationResponsePayload, [u8; KEY_LEN]) =
            self.unseal(local, envelope).await?;

        let status = if payload.accepted {
            let bundle = payload
                .key_bundle
                .as_ref()
                .ok_or(InboxError::MismatchedKeyBundle(payload.chat_id))?;

            bundle.verify(&sender_signing_key)?;

            if bundle.chat_id != payload.chat_id
                || bundle.user_id != envelope.sender_id
                || bundle.device_id != envelope.sender_device_id
            {
                return Err(InboxError::MismatchedKeyBundle(payload.chat_id));
            }

            self.exchanger
                .store_peer_key(
                    payload.chat_id,
                    envelope.sender_id,
                    envelope.sender_device_id,
                    bundle.chat_public_key.to_vec(),
                )
                .await?;

            let updated_chat = self
                .chat_repository
                .update_chat(UpdateChatInput {
                    id: payload.chat_id,
                    is_active: Some(true),
                    created_at: None,
                    updated_at: Some(Utc::now()),
                })
                .await?;

            let _ = self
                .events
                .send(FacadeEvent::ChatActivated(payload.chat_id));

            let latest = updated_chat.get_latest().map_err(InviteError::from)?;
            if latest.chat_type == ChatType::Group {
                let mut new_state = latest.clone();
                new_state
                    .members
                    .insert(envelope.sender_id, MemberRole::Member);
                self.chat_repository
                    .add_history_record(payload.chat_id, new_state)
                    .await?;

                let sender_nickname = self.user_repository.get(envelope.sender_id).await?.nickname;
                self.inviter
                    .announce_member_added(payload.chat_id, envelope.sender_id, sender_nickname)
                    .await?;
            }

            InvitationStatus::Accepted
        } else {
            InvitationStatus::Rejected
        };

        self.invitation_repository
            .resolve(payload.invitation_id, status)
            .await?;

        let _ = self.events.send(FacadeEvent::InvitationResolved {
            invitation_id: payload.invitation_id,
            status,
        });

        Ok(())
    }

    async fn handle_member_added(
        &self,
        local: &LocalIdentity,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let (payload, _): (MemberAddedPayload, [u8; KEY_LEN]) =
            self.unseal(local, envelope).await?;

        let chat = self.chat_repository.get(payload.chat_id).await?;
        let state = chat.get_latest().map_err(InviteError::from)?;

        let sender_role = state
            .members
            .get(&envelope.sender_id)
            .ok_or(InviteError::Forbidden(payload.chat_id))?;

        if !sender_role.can(Permissions::MANAGE_MEMBERS) {
            return Err(InviteError::Forbidden(payload.chat_id).into());
        }

        self.user_repository
            .upsert(User::contact(payload.user_id, payload.nickname.clone()))
            .await?;

        let mut new_state = state.clone();
        new_state
            .members
            .insert(payload.user_id, MemberRole::Member);
        self.chat_repository
            .add_history_record(payload.chat_id, new_state)
            .await?;

        let _ = self.events.send(FacadeEvent::MemberAdded {
            chat_id: payload.chat_id,
            user_id: payload.user_id,
        });

        self.inviter
            .announce_own_key(payload.chat_id, payload.user_id)
            .await?;

        Ok(())
    }

    async fn handle_chat_key_announcement(
        &self,
        local: &LocalIdentity,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let (bundle, sender_signing_key): (ChatKeyBundle, [u8; KEY_LEN]) =
            self.unseal(local, envelope).await?;

        // czat musi istnieć lokalnie, zeby obcy nie mogl zasiewac kluczy do
        // wymyslonych chat_id; to jedyna straz - zapis klucza nie wymaga
        // czlonkostwa nadawcy (patrz reguła rozwiazujaca wyscig w specyfikacji)
        let chat = self.chat_repository.get(bundle.chat_id).await?;

        bundle.verify(&sender_signing_key)?;

        if bundle.user_id != envelope.sender_id
            || bundle.device_id != envelope.sender_device_id
            || bundle.chat_id != envelope.chat_id
        {
            return Err(InboxError::MismatchedKeyBundle(bundle.chat_id));
        }

        self.exchanger
            .store_peer_key(
                bundle.chat_id,
                bundle.user_id,
                bundle.device_id,
                bundle.chat_public_key.to_vec(),
            )
            .await?;

        let state = chat.get_latest().map_err(InviteError::from)?;
        let other_members: Vec<Uuid> = state
            .members
            .keys()
            .filter(|member_id| **member_id != local.user_id)
            .copied()
            .collect();

        let total = other_members.len();
        let mut known = 0;
        for member_id in other_members {
            let keys = self
                .key_repository
                .list_by_chat_and_user(bundle.chat_id, member_id)
                .await?;
            if !keys.is_empty() {
                known += 1;
            }
        }

        let _ = self.events.send(FacadeEvent::KeyExchangeProgress {
            chat_id: bundle.chat_id,
            known,
            total,
        });

        Ok(())
    }

    async fn handle_delivery_receipt(
        &self,
        local: &LocalIdentity,
        envelope: &EncryptedEnvelope,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(
                envelope.chat_id,
                (local.user_id, local.device_id),
                (envelope.sender_id, envelope.sender_device_id),
            )
            .await?;

        let plaintext = Encryptor::decrypt_with_derived_key(&shared_key, &envelope.ciphertext)?;
        let receipt: DeliveryReceipt = serde_json::from_slice(&plaintext)?;

        let already_delivered = self
            .outbox_repository
            .any_delivered_for(receipt.message_id, envelope.sender_id)
            .await?;

        let entry = self
            .outbox_repository
            .get_by_message_recipient_device(
                receipt.message_id,
                envelope.sender_id,
                envelope.sender_device_id,
            )
            .await?;

        self.outbox_repository.mark_delivered(entry.id).await?;

        if !already_delivered {
            let _ = self.events.send(FacadeEvent::DeliveryReceiptMarked {
                message_id: receipt.message_id,
                recipient_id: envelope.sender_id,
            });
        }

        Ok(())
    }

    async fn send_delivery_receipt(
        &self,
        local: &LocalIdentity,
        envelope: &EncryptedEnvelope,
        message_id: Uuid,
    ) -> Result<(), InboxError> {
        let shared_key = self
            .exchanger
            .derive_shared_key(
                envelope.chat_id,
                (local.user_id, local.device_id),
                (envelope.sender_id, envelope.sender_device_id),
            )
            .await?;

        let receipt = DeliveryReceipt {
            id: Uuid::new_v4(),
            message_id,
            chat_id: envelope.chat_id,
            user_id: local.user_id,
        };

        let plaintext = serde_json::to_vec(&receipt)?;
        let ciphertext = Encryptor::encrypt_with_derived_key(&shared_key, &plaintext)?;

        let receipt_envelope = EncryptedEnvelope {
            id: Uuid::new_v4(),
            chat_id: envelope.chat_id,
            sender_id: local.user_id,
            sender_device_id: local.device_id,
            recipient_id: envelope.sender_id,
            recipient_device_id: Some(envelope.sender_device_id),
            created_at: Utc::now(),
            event_type: EventType::DeliveryReceipt,
            notify: false,
            ciphertext,
        };

        let outbox_entry = OutboxEntry::pending(
            Some(message_id),
            envelope.sender_id,
            &receipt_envelope,
            None,
        );

        self.outbox_repository
            .create_many(vec![outbox_entry])
            .await?;

        Ok(())
    }
}
