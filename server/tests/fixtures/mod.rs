#![allow(dead_code)]

use std::sync::Arc;

use server::core::initializers::redis::MailboxStore;
use server::core::initializers::settings::RedisSettings;
use server::domain::services::mailbox::Mailbox;

pub fn redis_settings() -> RedisSettings {
    RedisSettings {
        sentinel_nodes: vec![
            "localhost:26379".to_string(),
            "localhost:26380".to_string(),
            "localhost:26381".to_string(),
        ],
        service_name: "mymaster".to_string(),
        password: None,
    }
}

pub fn store() -> Arc<MailboxStore> {
    MailboxStore::new(&redis_settings())
        .expect("failed to build MailboxStore - is `make redis-up` running?")
}

pub fn mailbox() -> Arc<Mailbox> {
    Arc::new(Mailbox::new(store()))
}
