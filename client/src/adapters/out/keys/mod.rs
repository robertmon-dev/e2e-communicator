use uuid::Uuid;

pub enum DeviceKeyStoreError {
    NotImplemented = 0,
}

pub struct DeviceKeyStore {}

impl DeviceKeyStore {
    pub async fn get_or_create_keypair(
        &self,
        device_id: Uuid,
    ) -> Result<Vec<u8>, DeviceKeyStoreError> {
        Err(DeviceKeyStoreError::NotImplemented)
    }
}
