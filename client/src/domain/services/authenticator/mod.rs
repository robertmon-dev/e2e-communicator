use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::watch;
use uuid::Uuid;

use common::identity::{
    DevicePublicKeys, KEY_LEN, SIGNATURE_LEN, canonical_challenge_bytes, canonical_device_bytes,
    canonical_device_cert_bytes, sign, signing_public_key,
};

use crate::adapters::out::keys::{
    DeviceKeyStore, DeviceKeyStoreRepository, UserIdentityStore, UserIdentityStoreRepository,
};
use crate::adapters::out::persistance::repositories::local_identity::LocalIdentityRepository;
use crate::domain::dto::errors::{AuthError, TransportError};
use crate::domain::dto::identity::{IssuedLinkToken, LinkToken, RegisteredDevice, UserDirectory};
use crate::domain::dto::local_identity::LocalIdentity;
use crate::domain::state::connectivity::{ConnectivityState, ConnectivityStatus};
use crate::domain::state::identity::{IdentityState, IdentityStatus};
use crate::domain::state::session::{SessionState, SessionToken};

const DEFAULT_REFRESH_MARGIN: Duration = Duration::from_secs(10 * 60);
const DEFAULT_MAX_AUTH_FAILURES: u32 = 3;
const RETRY_CAP: Duration = Duration::from_secs(60);

const MIN_REFRESH_WAIT: Duration = Duration::from_secs(1);

#[async_trait]
pub trait IdentityTransport: Send + Sync {
    async fn register_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
        user_signing_key: &[u8; KEY_LEN],
        device_cert: &[u8; SIGNATURE_LEN],
    ) -> Result<(), TransportError>;

    async fn create_link_token(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<IssuedLinkToken, TransportError>;

    async fn link_device(
        &self,
        token: &str,
        device_id: Uuid,
        keys: &DevicePublicKeys,
        self_proof: &[u8; SIGNATURE_LEN],
        user_signing_key: &[u8; KEY_LEN],
        device_cert: &[u8; SIGNATURE_LEN],
    ) -> Result<Uuid, TransportError>;

    async fn revoke_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        target_device_id: Uuid,
    ) -> Result<(), TransportError>;

    async fn lookup_devices(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<UserDirectory, TransportError>;

    async fn challenge(&self, user_id: Uuid, device_id: Uuid) -> Result<[u8; 32], TransportError>;

    async fn authenticate(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        signature: &[u8; SIGNATURE_LEN],
    ) -> Result<SessionToken, TransportError>;
}

async fn wait_for_online(
    receiver: &mut watch::Receiver<ConnectivityStatus>,
) -> Result<(), watch::error::RecvError> {
    receiver
        .wait_for(|status| *status == ConnectivityStatus::Online)
        .await
        .map(|_| ())
}

async fn wait_for_registered(
    receiver: &mut watch::Receiver<IdentityStatus>,
) -> Result<(), watch::error::RecvError> {
    receiver
        .wait_for(|status| matches!(status, IdentityStatus::Registered(_)))
        .await
        .map(|_| ())
}

async fn wait_for_no_session(
    receiver: &mut watch::Receiver<Option<SessionToken>>,
) -> Result<(), watch::error::RecvError> {
    receiver.wait_for(|token| token.is_none()).await.map(|_| ())
}

pub struct Authenticator {
    transport: Arc<dyn IdentityTransport>,
    device_keys: Arc<DeviceKeyStore>,
    user_identity: Arc<UserIdentityStore>,
    identity_repository: Arc<LocalIdentityRepository>,
    identity: Arc<IdentityState>,
    session: Arc<SessionState>,
    connectivity: Arc<ConnectivityState>,
    refresh_margin: Duration,
    retry_delay: Option<Duration>,
    max_auth_failures: u32,
}

impl Authenticator {
    pub fn new(
        transport: Arc<dyn IdentityTransport>,
        device_keys: Arc<DeviceKeyStore>,
        user_identity: Arc<UserIdentityStore>,
        identity_repository: Arc<LocalIdentityRepository>,
        identity: Arc<IdentityState>,
        session: Arc<SessionState>,
        connectivity: Arc<ConnectivityState>,
    ) -> Self {
        Self {
            transport,
            device_keys,
            user_identity,
            identity_repository,
            identity,
            session,
            connectivity,
            refresh_margin: DEFAULT_REFRESH_MARGIN,
            retry_delay: None,
            max_auth_failures: DEFAULT_MAX_AUTH_FAILURES,
        }
    }

    pub fn with_refresh_margin(mut self, margin: Duration) -> Self {
        self.refresh_margin = margin;
        self
    }

    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = Some(delay);
        self
    }

    pub fn with_max_auth_failures(mut self, max: u32) -> Self {
        self.max_auth_failures = max.max(1);
        self
    }

    pub async fn register_new(&self) -> Result<LocalIdentity, AuthError> {
        self.require_unregistered()?;

        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        self.user_identity.get_or_create(user_id).await?;
        let keys = self.device_keys.get_or_create_keys(device_id).await?;
        let device_cert = self
            .user_identity
            .certify_device(user_id, device_id, &keys)
            .await?;

        let identity = self
            .identity_repository
            .create(user_id, device_id, device_cert.to_vec())
            .await?;
        self.identity
            .set(IdentityStatus::Registered(identity.clone()));

        tracing::info!(
            user_id = %identity.user_id,
            device_id = %identity.device_id,
            "new identity registered locally"
        );
        Ok(identity)
    }

    pub async fn link(
        &self,
        user_id: Uuid,
        token: &str,
        user_seed: &[u8; KEY_LEN],
    ) -> Result<LocalIdentity, AuthError> {
        self.require_unregistered()?;

        let device_id = Uuid::new_v4();
        let keys = self.device_keys.get_or_create_keys(device_id).await?;
        let proof = self
            .device_keys
            .sign(
                device_id,
                &canonical_device_bytes(user_id, device_id, &keys),
            )
            .await?;

        let user_signing_key = signing_public_key(user_seed);
        let device_cert = sign(
            user_seed,
            &canonical_device_cert_bytes(user_id, device_id, &keys),
        );

        let linked_user = self
            .transport
            .link_device(
                token,
                device_id,
                &keys,
                &proof,
                &user_signing_key,
                &device_cert,
            )
            .await?;
        if linked_user != user_id {
            return Err(AuthError::Transport(TransportError::ReceiveFailed(
                "server linked a different user id".to_string(),
            )));
        }

        self.user_identity.import_seed(user_id, user_seed).await?;

        let identity = self
            .identity_repository
            .create(user_id, device_id, device_cert.to_vec())
            .await?;
        self.identity
            .set(IdentityStatus::Registered(identity.clone()));

        tracing::info!(%user_id, %device_id, "device linked to existing user");
        Ok(identity)
    }

    pub async fn create_link_token(&self) -> Result<LinkToken, AuthError> {
        let identity = self.require_session()?;
        let issued = self
            .transport
            .create_link_token(identity.user_id, identity.device_id)
            .await?;
        let seed = self.user_identity.export_seed().await?;

        Ok(LinkToken {
            user_id: identity.user_id,
            token: issued.token,
            expires_at: issued.expires_at,
            user_seed: *seed,
        })
    }

    pub async fn list_devices(&self) -> Result<Vec<RegisteredDevice>, AuthError> {
        let identity = self.require_session()?;
        Ok(self
            .transport
            .lookup_devices(identity.user_id, identity.device_id, identity.user_id)
            .await?
            .devices)
    }

    pub async fn lookup_devices(&self, target_user_id: Uuid) -> Result<UserDirectory, AuthError> {
        let identity = self.require_session()?;
        Ok(self
            .transport
            .lookup_devices(identity.user_id, identity.device_id, target_user_id)
            .await?)
    }

    pub async fn revoke_device(&self, target_device_id: Uuid) -> Result<(), AuthError> {
        let identity = self.require_session()?;

        self.transport
            .revoke_device(identity.user_id, identity.device_id, target_device_id)
            .await?;

        if target_device_id == identity.device_id {
            self.session.clear();
            self.identity_repository.remove().await?;
            self.identity.set(IdentityStatus::Unregistered);
            tracing::info!(device_id = %target_device_id, "own device revoked, identity forgotten");
        } else {
            tracing::info!(device_id = %target_device_id, "device revoked");
        }

        Ok(())
    }

    pub async fn forget_identity(&self) -> Result<(), AuthError> {
        if matches!(self.identity.current(), IdentityStatus::Unregistered) {
            return Err(AuthError::WrongState);
        }

        let forgotten = self.identity.current().identity().cloned();

        self.session.clear();
        self.identity_repository.remove().await?;
        self.identity.set(IdentityStatus::Unregistered);

        match forgotten {
            Some(identity) => tracing::info!(
                user_id = %identity.user_id,
                device_id = %identity.device_id,
                "local identity forgotten"
            ),
            None => tracing::info!("local identity forgotten"),
        }

        Ok(())
    }

    pub async fn authenticate_once(
        &self,
        identity: &LocalIdentity,
    ) -> Result<SessionToken, AuthError> {
        let keys = self
            .device_keys
            .get_or_create_keys(identity.device_id)
            .await?;
        let proof = self
            .device_keys
            .sign(
                identity.device_id,
                &canonical_device_bytes(identity.user_id, identity.device_id, &keys),
            )
            .await?;

        let user_signing_key = self.user_identity.get_or_create(identity.user_id).await?;
        let device_cert = self
            .user_identity
            .certify_device(identity.user_id, identity.device_id, &keys)
            .await?;

        match self
            .transport
            .register_device(
                identity.user_id,
                identity.device_id,
                &keys,
                &proof,
                &user_signing_key,
                &device_cert,
            )
            .await
        {
            Ok(()) => {}
            Err(TransportError::AlreadyExists(_)) => return Err(AuthError::UserTaken),
            Err(other) => return Err(other.into()),
        }

        let nonce = self
            .transport
            .challenge(identity.user_id, identity.device_id)
            .await?;
        let signature = self
            .device_keys
            .sign(
                identity.device_id,
                &canonical_challenge_bytes(identity.user_id, identity.device_id, &nonce),
            )
            .await?;
        let token = self
            .transport
            .authenticate(identity.user_id, identity.device_id, &signature)
            .await?;

        self.session.set(token.clone());
        tracing::info!(expires_at = %token.expires_at, "session established");
        Ok(token)
    }

    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) {
        let mut connectivity_rx = self.connectivity.subscribe();
        let mut identity_rx = self.identity.subscribe();
        let mut session_rx = self.session.subscribe();
        let mut auth_failures: u32 = 0;
        let mut attempts: u32 = 0;

        loop {
            if *shutdown.borrow() {
                return;
            }

            if !self.connectivity.is_online() {
                tokio::select! {
                    _ = shutdown.changed() => return,
                    res = wait_for_online(&mut connectivity_rx) => {
                        if res.is_err() { return; }
                    }
                }
                continue;
            }

            let Some(identity) = self.identity.registered() else {
                auth_failures = 0;
                attempts = 0;
                tokio::select! {
                    _ = shutdown.changed() => return,
                    res = wait_for_registered(&mut identity_rx) => {
                        if res.is_err() { return; }
                    }
                }
                continue;
            };

            if let Some(token) = self.session.get() {
                let margin = chrono::Duration::from_std(self.refresh_margin)
                    .unwrap_or_else(|_| chrono::Duration::zero());
                let now = Utc::now();
                let remaining = (token.expires_at - now).to_std().unwrap_or(Duration::ZERO);
                let mut wait = (token.expires_at - margin - now)
                    .to_std()
                    .unwrap_or(Duration::ZERO);
                if remaining > MIN_REFRESH_WAIT && wait < MIN_REFRESH_WAIT {
                    wait = MIN_REFRESH_WAIT;
                }

                connectivity_rx.borrow_and_update();
                identity_rx.borrow_and_update();

                tokio::select! {
                    _ = shutdown.changed() => return,
                    _ = tokio::time::sleep(wait) => {
                        tracing::info!("Session nearing expiry, refreshing");
                        if let Err(err) = self.authenticate_once(&identity).await {
                            tracing::warn!(error = %err, "Session refresh failed, dropping session");
                            self.session.clear();
                        }
                    }
                    res = wait_for_no_session(&mut session_rx) => { if res.is_err() { return; } }
                    res = identity_rx.changed() => { if res.is_err() { return; } }
                    res = connectivity_rx.changed() => { if res.is_err() { return; } }
                }
                continue;
            }

            match self.authenticate_once(&identity).await {
                Ok(_) => {
                    auth_failures = 0;
                    attempts = 0;
                }
                Err(AuthError::UserTaken) => {
                    tracing::error!(
                        user_id = %identity.user_id,
                        "user id is registered to another device; identity is in conflict"
                    );
                    self.identity.set(IdentityStatus::Conflict(identity));
                    auth_failures = 0;
                    attempts = 0;
                }
                Err(AuthError::Transport(TransportError::Unauthenticated(reason))) => {
                    auth_failures += 1;
                    attempts += 1;
                    tracing::warn!(%reason, auth_failures, "Authentication rejected");
                    if auth_failures >= self.max_auth_failures {
                        tracing::error!(
                            device_id = %identity.device_id,
                            "Authentication rejected repeatedly; device is presumed revoked"
                        );
                        self.identity.set(IdentityStatus::Conflict(identity));
                        auth_failures = 0;
                        attempts = 0;
                    } else if !self.sleep_retry(attempts, &mut shutdown).await {
                        return;
                    }
                }
                Err(err) => {
                    attempts += 1;
                    tracing::warn!(error = %err, attempts, "Authentication attempt failed, retrying");
                    if !self.sleep_retry(attempts, &mut shutdown).await {
                        return;
                    }
                }
            }
        }
    }

    fn require_unregistered(&self) -> Result<(), AuthError> {
        match self.identity.current() {
            IdentityStatus::Unregistered => Ok(()),
            _ => Err(AuthError::WrongState),
        }
    }

    fn require_session(&self) -> Result<LocalIdentity, AuthError> {
        let identity = self.identity.registered().ok_or(AuthError::WrongState)?;
        if !self.session.is_authenticated() {
            return Err(AuthError::NoSession);
        }
        Ok(identity)
    }

    async fn sleep_retry(&self, attempts: u32, shutdown: &mut watch::Receiver<bool>) -> bool {
        let delay = self.retry_delay.unwrap_or_else(|| {
            common::backoff::exponential_with_jitter(attempts as i32, RETRY_CAP)
        });
        tokio::select! {
            _ = shutdown.changed() => false,
            _ = tokio::time::sleep(delay) => true,
        }
    }
}
