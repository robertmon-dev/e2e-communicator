use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    id: uuid::Uuid,
    name: String,
    surname: String,

    nickname: String,
    phone: Option<String>,
}
