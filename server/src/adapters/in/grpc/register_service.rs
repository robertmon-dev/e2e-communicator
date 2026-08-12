use std::sync::Arc;

use crate::domain::services::mailbox::Mailbox;

pub struct RegisterService {
    mailbox: Arc<Mailbox>,
}

impl RegisterService {
    pub fn new(mailbox: Arc<Mailbox>) -> Self {
        Self { mailbox }
    }
}
