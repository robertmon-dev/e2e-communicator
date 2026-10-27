use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::BTreeMap;
use std::convert::TryFrom;
use url::Url;
use uuid::Uuid;

use crate::adapters::out::persistance::entities::{
    file_metadata, profile_photo_history, user, user_overlay_history,
};
use crate::domain::dto::errors::ValueError;

#[derive(Debug, Clone, Copy, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ProfilePhotoType {
    Image = 0,
    Gif = 1,
}

impl TryFrom<u8> for ProfilePhotoType {
    type Error = ValueError;

    fn try_from(i: u8) -> Result<Self, ValueError> {
        match i {
            0 => Ok(Self::Image),
            1 => Ok(Self::Gif),
            _ => Err(ValueError::GreaterThanExpected),
        }
    }
}

impl From<ProfilePhotoType> for u8 {
    fn from(photo_type: ProfilePhotoType) -> Self {
        photo_type as u8
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfilePhoto {
    pub id: Uuid,

    pub user_id: Uuid,

    pub attachment_link: Option<Url>,
    pub profile_photo_type: ProfilePhotoType,

    pub file_name: String,
    pub file_size_bytes: i64,
    pub mime_type: Option<String>,

    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_secs: Option<i32>,

    pub thumbnail_link: Option<Url>,
}

impl ProfilePhoto {
    pub fn from(
        photo_model: profile_photo_history::Model,
        file_metadata_model: file_metadata::Model,
    ) -> Result<Self, ValueError> {
        let profile_photo_type = ProfilePhotoType::try_from(photo_model.profile_photo_type)?;

        let attachment_link = photo_model
            .attachment_link
            .map(|link| Url::parse(link.as_str()))
            .transpose()?;

        let thumbnail_link = file_metadata_model
            .thumbnail_link
            .map(|link| Url::parse(link.as_str()))
            .transpose()?;

        Ok(Self {
            id: photo_model.id,
            user_id: photo_model.user_id,
            attachment_link,
            profile_photo_type,
            file_name: file_metadata_model.file_name,
            file_size_bytes: file_metadata_model.file_size_bytes,
            mime_type: file_metadata_model.mime_type,
            width: file_metadata_model.width,
            height: file_metadata_model.height,
            duration_secs: file_metadata_model.duration_secs,
            thumbnail_link,
        })
    }

    pub fn into_model(
        &self,
        created_at: DateTime<Utc>,
    ) -> (profile_photo_history::Model, file_metadata::Model) {
        let attachment_link = self
            .attachment_link
            .as_ref()
            .map(|url| url.as_str().to_string());
        let thumbnail_link = self
            .thumbnail_link
            .as_ref()
            .map(|url| url.as_str().to_string());
        let file_metadata_id = Uuid::new_v4();

        let photo_model = profile_photo_history::Model {
            id: self.id,
            user_id: self.user_id,
            created_at,
            attachment_link,
            profile_photo_type: self.profile_photo_type.into(),
            file_metadata_id,
        };

        let file_metadata_model = file_metadata::Model {
            id: file_metadata_id,
            file_name: self.file_name.clone(),
            file_size_bytes: self.file_size_bytes,
            mime_type: self.mime_type.clone(),
            width: self.width,
            height: self.height,
            duration_secs: self.duration_secs,
            thumbnail_link,
            created_at,
        };

        (photo_model, file_metadata_model)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserOverlay {
    pub id: Uuid,

    pub user_id: Uuid,
    pub nickname: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub id: Uuid,

    pub name: Option<String>,
    pub surname: Option<String>,

    pub nickname: String,
    pub phone: Option<String>,

    pub created_at: DateTime<Utc>,

    pub signing_public_key: Option<Vec<u8>>,
    pub verified_at: Option<DateTime<Utc>>,

    pub history: BTreeMap<DateTime<Utc>, ProfilePhoto>,
    pub overlay: BTreeMap<DateTime<Utc>, Option<UserOverlay>>,
}

pub type UserPhotoHistory = Vec<(profile_photo_history::Model, file_metadata::Model)>;
pub type UserOverlayHistory = Vec<user_overlay_history::Model>;
pub type UnstructurizedUser = (user::Model, UserPhotoHistory, UserOverlayHistory);

impl User {
    pub fn new(
        name: Option<String>,
        surname: Option<String>,
        nickname: String,
        phone: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            surname,
            nickname,
            phone,
            created_at: Utc::now(),
            signing_public_key: None,
            verified_at: None,
            history: BTreeMap::new(),
            overlay: BTreeMap::new(),
        }
    }

    pub fn contact(id: Uuid, nickname: String) -> Self {
        Self {
            id,
            name: None,
            surname: None,
            nickname,
            phone: None,
            created_at: Utc::now(),
            signing_public_key: None,
            verified_at: None,
            history: BTreeMap::new(),
            overlay: BTreeMap::new(),
        }
    }

    pub fn from(
        user_model: user::Model,
        photo_history: UserPhotoHistory,
        overlay_history: UserOverlayHistory,
    ) -> Result<Self, ValueError> {
        let mut history = BTreeMap::new();
        for (photo_model, file_metadata_model) in photo_history {
            let created_at = photo_model.created_at;
            history.insert(
                created_at,
                ProfilePhoto::from(photo_model, file_metadata_model)?,
            );
        }

        let mut overlay = BTreeMap::new();
        for overlay_model in overlay_history {
            let user_overlay = overlay_model.nickname.map(|nickname| UserOverlay {
                id: overlay_model.id,
                user_id: overlay_model.user_id,
                nickname,
            });
            overlay.insert(overlay_model.created_at, user_overlay);
        }

        Ok(Self {
            id: user_model.id,
            name: user_model.name,
            surname: user_model.surname,
            nickname: user_model.nickname,
            phone: user_model.phone,
            created_at: user_model.created_at,
            signing_public_key: user_model.signing_public_key,
            verified_at: user_model.verified_at,
            history,
            overlay,
        })
    }
}

impl From<User> for UnstructurizedUser {
    fn from(user: User) -> Self {
        let user_model = user::Model {
            id: user.id,
            name: user.name,
            surname: user.surname,
            nickname: user.nickname,
            phone: user.phone,
            created_at: user.created_at,
            updated_at: Utc::now(),
            signing_public_key: user.signing_public_key,
            verified_at: user.verified_at,
        };

        let photo_history = user
            .history
            .into_iter()
            .map(|(created_at, photo)| photo.into_model(created_at))
            .collect();

        let overlay_history = user
            .overlay
            .into_iter()
            .map(|(created_at, overlay)| {
                let id = overlay.as_ref().map(|o| o.id).unwrap_or_else(Uuid::new_v4);
                let nickname = overlay.map(|o| o.nickname);

                user_overlay_history::Model {
                    id,
                    user_id: user.id,
                    nickname,
                    created_at,
                }
            })
            .collect();

        (user_model, photo_history, overlay_history)
    }
}
