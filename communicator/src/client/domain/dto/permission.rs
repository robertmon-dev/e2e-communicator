use bitflags::bitflags;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MemberRole {
    Member,
    Moderator,
    Admin,
    Custom(Permissions),
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct Permissions: u8 {
        const SEND_MESSAGES = 0b00000001;
        const EDIT_MESSAGES = 0b00000010;
        const MANAGE_MEMBERS = 0b00000100;
        const CHANGE_NAME    = 0b00001000;
        const CHANGE_THEME = 0b00010000;

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
