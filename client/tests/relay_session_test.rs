mod fixtures;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use client::adapters::out::persistance::repositories::outbox::OutboxRepository;
use client::adapters::out::persistance::repositories::outbox::create::Create as OutboxCreate;
use client::domain::dto::errors::TransportError;
use client::domain::dto::outbox::{OutboxEntry, OutboxStatus};
use client::domain::services::relay::{EnvelopeTransport, Relay};
use client::domain::state::connectivity::ConnectivityState;
use client::domain::state::session::{SessionState, SessionToken};
use common::envelope::{EncryptedEnvelope, EventType};
use fixtures::{seed_chat, seed_message, seed_user, setup_db};

struct RecordingTransport {
    sent: Mutex<Vec<Uuid>>,
    fail_once_with: Mutex<Option<TransportError>>,
}

impl RecordingTransport {
    fn new(fail_once_with: Option<TransportError>) -> Arc<Self> {
        Arc::new(Self {
            sent: Mutex::new(Vec::new()),
            fail_once_with: Mutex::new(fail_once_with),
        })
    }

    fn sent(&self) -> Vec<Uuid> {
        self.sent.lock().unwrap().clone()
    }
}

impl EnvelopeTransport for RecordingTransport {
    async fn send(&self, entry: &OutboxEntry) -> Result<(), TransportError> {
        if let Some(err) = self.fail_once_with.lock().unwrap().take() {
            return Err(err);
        }

        self.sent.lock().unwrap().push(entry.id);
        Ok(())
    }
}

fn session_token() -> SessionToken {
    SessionToken {
        token: "test-session".to_string(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    }
}

async fn seed_pending_entry(
    db: &Arc<sea_orm::DatabaseConnection>,
) -> (Arc<OutboxRepository>, Uuid) {
    let sender_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    let chat_id = Uuid::new_v4();
    let message_id = Uuid::new_v4();

    seed_user(db, sender_id).await;
    seed_user(db, recipient_id).await;
    seed_chat(db, chat_id).await;
    seed_message(db, message_id, sender_id, chat_id).await;

    let envelope = EncryptedEnvelope {
        id: Uuid::new_v4(),
        chat_id,
        sender_id,
        recipient_id,
        created_at: Utc::now(),
        event_type: EventType::Message,
        ciphertext: vec![1, 2, 3],
    };
    let entry = OutboxEntry::pending(message_id, recipient_id, &envelope, None);
    let entry_id = entry.id;

    let outbox = Arc::new(OutboxRepository::new(db.clone()));
    outbox.create_many(vec![entry]).await.unwrap();

    (outbox, entry_id)
}

async fn wait_until<F: Fn() -> bool>(what: &str, condition: F) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for: {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn relay_waits_for_a_session_before_sending() {
    let db = setup_db().await;
    let (outbox, entry_id) = seed_pending_entry(&db).await;
    let transport = RecordingTransport::new(None);
    let session = Arc::new(SessionState::default());

    let relay = Relay::new(
        outbox.clone(),
        Arc::new(ConnectivityState::default()),
        session.clone(),
        transport.clone(),
    )
    .with_idle_interval(Duration::from_millis(20));

    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runner = tokio::spawn(async move { relay.run(shutdown_rx).await });

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        transport.sent().is_empty(),
        "nothing may be sent without a session"
    );
    assert_eq!(
        outbox.get(entry_id).await.unwrap().status,
        OutboxStatus::Pending
    );

    session.set(session_token());
    wait_until("the entry to be sent", || {
        transport.sent().contains(&entry_id)
    })
    .await;

    let entry = outbox.get(entry_id).await.unwrap();
    assert_eq!(entry.status, OutboxStatus::Sent);

    runner.abort();
}

#[tokio::test]
async fn an_unauthenticated_send_clears_the_session_and_keeps_the_entry_pending() {
    let db = setup_db().await;
    let (outbox, entry_id) = seed_pending_entry(&db).await;
    let transport = RecordingTransport::new(Some(TransportError::Unauthenticated(
        "session expired".to_string(),
    )));
    let session = Arc::new(SessionState::default());
    session.set(session_token());

    let relay = Relay::new(
        outbox.clone(),
        Arc::new(ConnectivityState::default()),
        session.clone(),
        transport.clone(),
    )
    .with_idle_interval(Duration::from_millis(20));

    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runner = tokio::spawn(async move { relay.run(shutdown_rx).await });

    wait_until("the session to be cleared", || session.get().is_none()).await;

    let entry = outbox.get(entry_id).await.unwrap();
    assert_eq!(
        entry.status,
        OutboxStatus::Pending,
        "an unauthenticated send must not consume a retry"
    );
    assert_eq!(entry.attempts, 0);
    assert!(transport.sent().is_empty());

    session.set(session_token());
    wait_until("delivery after re-authentication", || {
        transport.sent().contains(&entry_id)
    })
    .await;

    runner.abort();
}
