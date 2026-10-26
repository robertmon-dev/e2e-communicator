use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{Mutex, RwLock, watch};
use tokio::task::JoinHandle;

use migration::connection::derive_boot_keys;
use zeroize::Zeroizing;

use crate::adapters::r#in::ipc::facade::params::VaultUnlockParams;
use crate::adapters::r#in::ipc::facade::utils::parse;
use crate::adapters::r#in::ipc::facade::{Facade, verify_handshake};
use crate::adapters::r#in::ipc::protocol::{Outcome, Request, Response, RpcError};
use crate::core::bootstrap::{self, BootError, Started};
use crate::core::initializers::database::settings::DatabaseSettings;
use crate::core::initializers::settings::Settings;
use crate::domain::dto::errors::VaultError;
use crate::domain::state::facade_events::{FacadeEvent, FacadeEventBus};
use crate::domain::state::vault::{VaultState, VaultStatus};

pub const CODE_LOCKED: i32 = -32009;
pub const CODE_BAD_PIN: i32 = -32010;

const UNLOCK_BASE_DELAY: Duration = Duration::from_millis(500);
const UNLOCK_MAX_DELAY: Duration = Duration::from_secs(30);
const UNLOCK_MAX_FAILURES: u32 = 10;

pub struct Gate {
    boot: Option<BootContext>,
    session_token: Option<String>,
    vault: Arc<VaultState>,
    failures: Mutex<u32>,
    inner: RwLock<Option<Started>>,
}

struct BootContext {
    settings: Arc<Settings>,
    events: FacadeEventBus,
    shutdown: watch::Receiver<bool>,
}

impl Gate {
    pub fn locked(
        settings: Arc<Settings>,
        events: FacadeEventBus,
        shutdown: watch::Receiver<bool>,
        vault: Arc<VaultState>,
    ) -> Self {
        Self {
            session_token: settings.facade.session_token.clone(),
            boot: Some(BootContext {
                settings,
                events,
                shutdown,
            }),
            vault,
            failures: Mutex::new(0),
            inner: RwLock::new(None),
        }
    }

    pub async fn unlocked(facade: Arc<Facade>, session_token: Option<String>) -> Self {
        let gate = Self {
            boot: None,
            session_token,
            vault: Arc::new(VaultState::unlocked()),
            failures: Mutex::new(0),
            inner: RwLock::new(None),
        };

        gate.inner.write().await.replace(Started {
            facade,
            handles: Vec::new(),
        });

        gate
    }

    pub fn vault_status(&self) -> VaultStatus {
        self.vault.current()
    }

    async fn install(&self, started: Started) {
        self.inner.write().await.replace(started);
        self.vault.set(VaultStatus::Unlocked);

        if let Some(boot) = &self.boot {
            let _ = boot
                .events
                .send(FacadeEvent::VaultChanged(VaultStatus::Unlocked));
        }
    }

    pub async fn take_handles(&self) -> Vec<JoinHandle<()>> {
        match self.inner.write().await.as_mut() {
            Some(started) => std::mem::take(&mut started.handles),
            None => Vec::new(),
        }
    }

    pub async fn authenticate_session(&self, params: Value) -> Result<Value, RpcError> {
        if let Some(started) = self.inner.read().await.as_ref() {
            let response = started.facade.authenticate_session(params)?;
            return Ok(serde_json::to_value(response).expect("SessionInit always serializes"));
        }

        let params = verify_handshake(params, self.session_token.as_deref())?;

        Ok(serde_json::json!({
            "protocol_version": params.protocol_version,
            "vault_state": VaultStatus::Locked.label(),
            "identity_state": "locked",
            "user_id": Value::Null,
            "device_id": Value::Null,
            "connectivity": "unknown",
            "authenticated": false,
        }))
    }

    pub async fn handle(&self, request: Request) -> Response {
        let outcome = match request.method.as_str() {
            "vault.status" => Outcome::Ok {
                result: serde_json::json!({ "state": self.vault_status().label() }),
            },
            "vault.unlock" => self.handle_unlock(request.params).await,
            _ => match self.inner.read().await.as_ref() {
                Some(started) => {
                    return started
                        .facade
                        .handle(Request {
                            id: request.id,
                            method: request.method,
                            params: request.params,
                        })
                        .await;
                }
                None => Outcome::Err {
                    error: RpcError {
                        code: CODE_LOCKED,
                        message: "the vault is locked: call vault.unlock first".to_string(),
                    },
                },
            },
        };

        Response {
            id: request.id,
            outcome,
        }
    }

    async fn handle_unlock(&self, params: Value) -> Outcome {
        let params: VaultUnlockParams = match parse(params) {
            Ok(params) => params,
            Err(outcome) => return outcome,
        };

        let pin = Zeroizing::new(params.pin);

        match self.unlock(&pin).await {
            Ok(()) => Outcome::Ok {
                result: serde_json::json!({ "state": VaultStatus::Unlocked.label() }),
            },
            Err(err) => Outcome::Err {
                error: vault_error(err),
            },
        }
    }

    pub async fn unlock(&self, pin: &str) -> Result<(), VaultError> {
        if self.vault.is_unlocked() {
            return Err(VaultError::AlreadyUnlocked);
        }

        let Some(boot) = &self.boot else {
            return Err(VaultError::AlreadyUnlocked);
        };

        let mut failures = self.failures.lock().await;
        if *failures >= UNLOCK_MAX_FAILURES {
            return Err(VaultError::TooManyAttempts);
        }

        DatabaseSettings::validate_pin(pin).map_err(VaultError::BadPinFormat)?;

        let Some(salt) = boot.settings.database.salt.clone() else {
            return Err(VaultError::Boot("Missing database salt".to_string()));
        };

        if *failures > 0 {
            let delay = UNLOCK_BASE_DELAY
                .saturating_mul(1 << (*failures - 1).min(16))
                .min(UNLOCK_MAX_DELAY);
            tokio::time::sleep(delay).await;
        }

        let owned_pin = Zeroizing::new(pin.to_string());
        let derived = tokio::task::spawn_blocking(move || {
            derive_boot_keys(&owned_pin, &salt).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| VaultError::Boot(format!("key derivation task failed: {e}")))?;

        let boot_keys = match derived {
            Ok(keys) => keys,
            Err(reason) => {
                *failures += 1;
                tracing::warn!(%reason, failures = *failures, "Vault unlock rejected");
                return Err(VaultError::WrongPin);
            }
        };

        match bootstrap::start(
            boot.settings.clone(),
            boot_keys,
            boot.events.clone(),
            boot.shutdown.clone(),
        )
        .await
        {
            Ok(started) => {
                *failures = 0;
                self.install(started).await;
                tracing::info!("Vault unlocked; client services started");
                Ok(())
            }
            Err(reason) => {
                *failures += 1;
                tracing::warn!(%reason, failures = *failures, "Vault unlock failed");
                Err(match reason {
                    BootError::WrongKey => VaultError::WrongPin,
                    BootError::Failed(reason) => VaultError::Boot(reason),
                })
            }
        }
    }
}

fn vault_error(err: VaultError) -> RpcError {
    let code = match err {
        VaultError::AlreadyUnlocked => CODE_LOCKED,
        VaultError::BadPinFormat(_) | VaultError::WrongPin | VaultError::TooManyAttempts => {
            CODE_BAD_PIN
        }
        VaultError::Boot(_) => -32000,
    };

    RpcError {
        code,
        message: err.to_string(),
    }
}
