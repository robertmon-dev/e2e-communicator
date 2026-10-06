use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct UserUpdateInput {
    pub id: Uuid,

    pub name: Option<String>,
    pub surname: Option<String>,
    pub nickname: Option<String>,
    pub phone: Option<String>,
}
