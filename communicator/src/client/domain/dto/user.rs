use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProfilePhotoType {
    Image = 0,
    Gif = 1,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProfilePhoto {
    pub id: Uuid,

    pub user_id: Uuid,

    pub attachment_link: Option<Url>,
    pub profile_photo_type: ProfilePhotoType,

    pub file_name: String,
    pub file_size_bytes: u64,
    pub mime_type: Option<String>,

    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_secs: Option<u32>,

    pub thumbnail_link: Option<Url>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    id: uuid::Uuid,
    name: String,
    surname: String,

    nickname: String,
    phone: Option<String>,

    profile_photo: Option<ProfilePhotoType>,
}
