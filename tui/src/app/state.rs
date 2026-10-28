use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::ipc::protocol::{
    ChatView, ContactCodeView, DeviceView, IdentityInfo, InvitationView, LinkTokenView,
    MessageView, SafetyNumberView,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Stream,
    Devices,
    LinkToken,
    Unlock,
    Conversation(Uuid),
    Invitations,
    ContactCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpPlacement {
    Left,
    Centre,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Chats,
    Main,
    Command,
}

impl Focus {
    pub const ORDER: [Self; 3] = [Self::Chats, Self::Main, Self::Command];

    pub fn step(self, forward: bool) -> Self {
        let at = Self::ORDER
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);

        let next = if forward {
            (at + 1) % Self::ORDER.len()
        } else {
            (at + Self::ORDER.len() - 1) % Self::ORDER.len()
        };

        Self::ORDER[next]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Idle,
    Command(String),
    Message(String),
    Pin(String),
}

impl InputMode {
    pub fn buffer(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Command(buffer) | Self::Message(buffer) | Self::Pin(buffer) => Some(buffer),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingKind {
    SessionInit,
    VaultUnlock,
    IdentityGet,
    IdentityRegister,
    IdentityLink,
    IdentityForget,
    DevicesList,
    LinkToken,
    DeviceRevoke(Uuid),
    ChatsList,
    MessagesList(Uuid),
    MessageSend,
    ContactCode,
    InvitationCreate,
    InvitationsList,
    InvitationAccept(Uuid),
    InvitationReject(Uuid),
    InvitationCancel(Uuid),
    GroupCreate,
    SafetyNumber(Uuid),
    ContactVerify(Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub prompt: String,
    pub kind: ConfirmKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    RevokeDevice(Uuid),
    ForgetIdentity,
    RejectInvitation(Uuid),
    VerifyContact(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Info,
    Outgoing,
    Incoming,
    Problem,
}

#[derive(Debug, Clone)]
pub struct StreamLine {
    pub at: DateTime<Utc>,
    pub kind: StreamKind,
    pub text: String,
}

pub struct App {
    pub connection: ConnectionStatus,
    pub identity: IdentityInfo,
    pub authenticated: bool,
    pub connectivity: String,
    pub protocol_mismatch: bool,

    pub view: View,
    pub locked: bool,
    pub focus: Focus,
    pub help: Option<HelpPlacement>,
    pub input: InputMode,
    pub confirm: Option<Confirm>,

    pub stream: VecDeque<StreamLine>,
    pub raw_stream: bool,

    pub devices: Vec<DeviceView>,
    pub device_cursor: usize,
    pub link_token: Option<LinkTokenView>,

    pub chats: Vec<ChatView>,
    pub chat_cursor: usize,
    pub messages: Vec<MessageView>,

    pub invitations: Vec<InvitationView>,
    pub invitation_cursor: usize,
    pub contact_code: Option<ContactCodeView>,
    pub key_exchange: HashMap<Uuid, (usize, usize)>,
    pub safety_number: Option<SafetyNumberView>,

    pub pending: HashMap<u64, PendingKind>,
    next_id: u64,
    pub now: DateTime<Utc>,
    pub should_quit: bool,
}

impl App {
    pub const STREAM_CAPACITY: usize = 500;

    pub fn new() -> Self {
        Self {
            connection: ConnectionStatus::Disconnected {
                reason: crate::i18n::t("conn.not_connected_yet"),
            },
            identity: IdentityInfo::default(),
            authenticated: false,
            connectivity: crate::i18n::t("status.connectivity_unknown"),
            protocol_mismatch: false,
            view: View::Stream,
            locked: false,
            focus: Focus::Main,
            help: None,
            input: InputMode::Idle,
            confirm: None,
            stream: VecDeque::new(),
            raw_stream: false,
            devices: Vec::new(),
            device_cursor: 0,
            link_token: None,
            chats: Vec::new(),
            chat_cursor: 0,
            messages: Vec::new(),
            invitations: Vec::new(),
            invitation_cursor: 0,
            contact_code: None,
            key_exchange: HashMap::new(),
            safety_number: None,
            pending: HashMap::new(),
            next_id: 1,
            now: Utc::now(),
            should_quit: false,
        }
    }

    pub fn track(&mut self, kind: PendingKind) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, kind);
        id
    }

    pub fn log(&mut self, kind: StreamKind, text: impl Into<String>) {
        self.stream.push_back(StreamLine {
            at: self.now,
            kind,
            text: text.into(),
        });

        while self.stream.len() > Self::STREAM_CAPACITY {
            self.stream.pop_front();
        }
    }

    pub fn link_token_remaining(&self) -> Option<Duration> {
        self.link_token
            .as_ref()
            .map(|token| (token.expires_at - self.now).max(Duration::zero()))
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.connection, ConnectionStatus::Connected)
    }

    pub fn focused_chat(&self) -> Option<Uuid> {
        match self.view {
            View::Conversation(chat_id) => Some(chat_id),
            _ => None,
        }
    }

    pub fn upsert_invitation(&mut self, view: InvitationView) {
        match self
            .invitations
            .iter_mut()
            .find(|existing| existing.id == view.id)
        {
            Some(existing) => *existing = view,
            None => self.invitations.insert(0, view),
        }
    }

    pub fn resolve_invitation(&mut self, invitation_id: Uuid, status: u8) {
        if let Some(invitation) = self
            .invitations
            .iter_mut()
            .find(|existing| existing.id == invitation_id)
        {
            invitation.status = status;
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
