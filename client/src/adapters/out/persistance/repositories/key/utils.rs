use uuid::Uuid;

pub struct KeyUpdateInput {
    pub id: Uuid,

    pub public_key: Option<Vec<u8>>,
    pub encrypted_private_key: Option<Vec<u8>>,
}
