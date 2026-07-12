use bitflags::bitflags;
use serde::{Deserialize, Serialize};

use crate::client::domain::dto::errors::ValueError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MemberRole {
    Member,
    Moderator,
    Admin,
    Custom(Permissions),
}

impl MemberRole {
    pub fn from(n: u8) -> Result<Self, ValueError> {
        match n {
            0 => Ok(Self::Member),
            1 => Ok(Self::Moderator),
            2 => Ok(Self::Admin),
            n if n > 2 => {
                let flags = Permissions::from_bits_truncate(n);
                Ok(Self::Custom(flags))
            }
            _ => Err(ValueError::GreaterThanExpected),
        }
    }

    pub fn into_u8(&self) -> u8 {
        self.into_u8()
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct Permissions: u8 {
        const SEND_MESSAGES = 0b00000001;
        const EDIT_MESSAGES = 0b00000010;
        const MANAGE_MEMBERS = 0b00000100;
        const CHANGE_NAME    = 0b00001000;
        const CHANGE_THEME = 0b00010000;
        const REMOVE_GROUP = 0b00100000;
    }
}

impl MemberRole {
    pub fn can(&self, permission: Permissions) -> bool {
        match self {
            MemberRole::Admin => true,
            MemberRole::Moderator => {
                permission.contains(Permissions::SEND_MESSAGES)
                    || permission.contains(Permissions::EDIT_MESSAGES)
            }
            MemberRole::Member => permission == Permissions::SEND_MESSAGES,
            MemberRole::Custom(p) => p.contains(permission),
        }
    }
}
