use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, WriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;

use crate::domain::state::facade_events::FacadeEventBus;

use super::facade::Facade;
use super::protocol::{Notification, Outcome, Request, Response, RpcError};

pub fn default_socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("e2e-communicator-client.sock")
}

pub async fn run(
    socket_path: &Path,
    facade: Arc<Facade>,
    events: FacadeEventBus,
) -> io::Result<()> {
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o700))?;

    loop {
        let (stream, _addr) = listener.accept().await?;
        if let Ok(cred) = stream.peer_cred() {
            let current_uid = unsafe { libc::getuid() };
            if cred.uid() != current_uid {
                tracing::warn!(
                    peer_uid = cred.uid(),
                    expected_uid = current_uid,
                    "Unauthorized connection attempt from different user, dropping."
                );
                continue;
            }
        } else {
            tracing::warn!("Failed to get peer credentials, dropping connection.");
            continue;
        }

        tokio::spawn(handle_connection(stream, facade.clone(), events.clone()));
    }
}

async fn handle_connection(stream: UnixStream, facade: Arc<Facade>, events: FacadeEventBus) {
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut lines = BufReader::new(read_half).lines();
    let mut receiver = events.subscribe();

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let line = match line {
                    Ok(Some(line)) => line,
                    _ => break,
                };
                let response = match serde_json::from_str::<Request>(&line) {
                    Ok(request) => facade.handle(request).await,
                    Err(e) => Response {
                        id: serde_json::Value::Null,
                        outcome: Outcome::Err {
                            error: RpcError { code: -32700, message: format!("parse error: {e}") },
                        },
                    },
                };
                if write_json(&mut write_half, &response).await.is_err() {
                    break;
                }
            }

            event = receiver.recv() => {
                match event {
                    Ok(event) => {
                        let notification: Notification = event.into();
                        if write_json(&mut write_half, &notification).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("facade event forwarder lagged, skipped {skipped} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn write_json<T: serde::Serialize>(
    write_half: &mut WriteHalf<UnixStream>,
    value: &T,
) -> io::Result<()> {
    let Ok(json) = serde_json::to_string(value) else {
        return Ok(());
    };
    write_half.write_all(json.as_bytes()).await?;
    write_half.write_all(b"\n").await
}
