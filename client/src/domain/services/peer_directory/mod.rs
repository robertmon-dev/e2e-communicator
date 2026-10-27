use std::sync::Arc;

use uuid::Uuid;

use common::identity::{
    DevicePublicKeys, FINGERPRINT_LEN, KEY_LEN, canonical_device_cert_bytes, user_fingerprint,
    verify,
};

use crate::adapters::out::persistance::repositories::peer_device::{
    PeerDeviceRecord, PeerDeviceRepository,
};
use crate::domain::dto::errors::PeerDirectoryError;
use crate::domain::services::authenticator::IdentityTransport;
use crate::domain::state::identity::IdentityState;

pub struct VerifiedDevice {
    pub device_id: Uuid,
    pub keys: DevicePublicKeys,
}

pub struct ResolvedPeer {
    pub user_id: Uuid,
    pub user_signing_key: [u8; KEY_LEN],
    pub devices: Vec<VerifiedDevice>,
}

pub struct PeerDirectory {
    transport: Arc<dyn IdentityTransport>,
    peer_devices: Arc<PeerDeviceRepository>,
    identity: Arc<IdentityState>,
}

impl PeerDirectory {
    pub fn new(
        transport: Arc<dyn IdentityTransport>,
        peer_devices: Arc<PeerDeviceRepository>,
        identity: Arc<IdentityState>,
    ) -> Self {
        Self {
            transport,
            peer_devices,
            identity,
        }
    }

    pub async fn resolve(
        &self,
        peer_id: Uuid,
        expected_fingerprint: Option<[u8; FINGERPRINT_LEN]>,
    ) -> Result<ResolvedPeer, PeerDirectoryError> {
        let local = self
            .identity
            .registered()
            .ok_or(PeerDirectoryError::Unregistered)?;

        let directory = self
            .transport
            .lookup_devices(local.user_id, local.device_id, peer_id)
            .await?;

        if let Some(expected) = expected_fingerprint
            && user_fingerprint(&directory.user_signing_key) != expected
        {
            return Err(PeerDirectoryError::IdentityMismatch(peer_id));
        }

        if let Some(pinned) = self.peer_devices.pinned_identity(peer_id).await?
            && pinned != directory.user_signing_key.to_vec()
        {
            return Err(PeerDirectoryError::IdentityChanged(peer_id));
        }

        let mut verified = Vec::new();
        let mut records = Vec::new();

        for device in &directory.devices {
            let message = canonical_device_cert_bytes(peer_id, device.device_id, &device.keys);

            match verify(&directory.user_signing_key, &message, &device.cert) {
                Ok(()) => {
                    verified.push(VerifiedDevice {
                        device_id: device.device_id,
                        keys: device.keys.clone(),
                    });
                    records.push(PeerDeviceRecord {
                        user_id: peer_id,
                        device_id: device.device_id,
                        signing_public_key: device.keys.signing_key.to_vec(),
                        agreement_public_key: device.keys.agreement_key.to_vec(),
                        device_cert: device.cert.to_vec(),
                    });
                }
                Err(err) => {
                    tracing::warn!(
                        peer_id = %peer_id,
                        device_id = %device.device_id,
                        error = ?err,
                        "dropping peer device with an invalid certificate"
                    );
                }
            }
        }

        if verified.is_empty() {
            return Err(PeerDirectoryError::NoVerifiedDevices(peer_id));
        }

        self.peer_devices
            .pin_identity(peer_id, directory.user_signing_key.to_vec())
            .await?;
        self.peer_devices.upsert_many(records).await?;

        Ok(ResolvedPeer {
            user_id: peer_id,
            user_signing_key: directory.user_signing_key,
            devices: verified,
        })
    }

    pub async fn cached(&self, peer_id: Uuid) -> Result<Vec<VerifiedDevice>, PeerDirectoryError> {
        let models = self.peer_devices.list_by_user(peer_id).await?;

        if models.is_empty() {
            return Err(PeerDirectoryError::NoVerifiedDevices(peer_id));
        }

        models
            .into_iter()
            .map(|model| {
                let keys = DevicePublicKeys::from_slices(
                    &model.signing_public_key,
                    &model.agreement_public_key,
                )
                .map_err(|_| PeerDirectoryError::NoVerifiedDevices(peer_id))?;

                Ok(VerifiedDevice {
                    device_id: model.device_id,
                    keys,
                })
            })
            .collect()
    }
}
