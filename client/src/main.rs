use std::sync::Arc;

use client::adapters::r#in::ipc::gate::Gate;
use client::adapters::r#in::ipc::server as ipc_server;
use client::core::initializers::logger;
use client::core::initializers::settings::{AppEnv, Settings};
use client::domain::state::facade_events::facade_event_bus;
use client::domain::state::vault::VaultState;
use tokio::sync::watch;

#[tokio::main]
async fn main() {
    let (shutdown_tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();

        #[cfg(unix)]
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => signal.recv().await,
                Err(e) => {
                    tracing::warn!("Failed to register hanlder for SIGTERM: {e}");
                    std::future::pending::<Option<()>>().await
                }
            }
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            res = ctrl_c => {
                if let Err(e) = res {
                    tracing::error!("Error while listening for SIGINT: {e}");
                } else {
                    tracing::info!("Received signal for SIGINT. Shutting down.");
                }
            }
            _ = terminate => {
                tracing::info!("Received signal for SIGTERM. Shutting down.");
            }
        }

        let _ = shutdown_tx.send(true);
    });

    let settings = Arc::new(Settings::new().expect("Failed to load .env variables"));

    logger::init(&settings);
    tracing::info!("Booting application...");

    if settings.database.salt.is_none() {
        tracing::error!("Missing database salt");
        std::process::exit(1);
    }

    let events = facade_event_bus();
    let vault = Arc::new(VaultState::default());

    let gate = Arc::new(Gate::locked(
        settings.clone(),
        events.clone(),
        rx.clone(),
        vault,
    ));

    match (&settings.environment, &settings.database.encryption_key) {
        (AppEnv::Test, Some(pin)) => match gate.unlock(pin).await {
            Ok(()) => tracing::info!("Vault unlocked from the environment (test mode)"),
            Err(e) => {
                tracing::error!("Environment PIN was refused: {e}");
                std::process::exit(1);
            }
        },
        (_, Some(_)) => tracing::warn!(
            "Ignoring APP__DATABASE__ENCRYPTION_KEY outside the test environment: \
             send the PIN through vault.unlock instead"
        ),
        (_, None) => {}
    }

    if !gate.vault_status().is_unlocked() {
        tracing::info!("Vault is locked: waiting for vault.unlock over the IPC facade");
    }

    let socket_path = settings
        .facade
        .socket_path
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(ipc_server::default_socket_path);

    if let Err(e) = ipc_server::run(&socket_path, gate.clone(), events, rx).await {
        tracing::error!("IPC server failed: {e}");
    }

    for handle in gate.take_handles().await {
        let _ = handle.await;
    }
}
