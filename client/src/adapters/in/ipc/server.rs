use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines, Take, WriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{self, broadcast};
use tokio::time::{Duration, timeout};

use crate::domain::state::facade_events::FacadeEventBus;

use super::facade::Facade;
use super::protocol::{Notification, Outcome, Request, Response, RpcError};

pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
pub const MAX_MESSAGE_SIZE_BYTES: u64 = 2 * 1024 * 1024;

type ClientReader = Lines<BufReader<Take<tokio::io::ReadHalf<UnixStream>>>>;
type ClientWriter = WriteHalf<UnixStream>;

pub fn default_socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("e2e-communicator-client.sock")
}

pub async fn run(
    socket_path: &Path,
    facade: Arc<Facade>,
    events: FacadeEventBus,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> io::Result<()> {
    let _ = std::fs::remove_file(socket_path);
    let listener = {
        let prev_mask = unsafe { libc::umask(0o077) };
        let res = UnixListener::bind(socket_path);
        unsafe { libc::umask(prev_mask) };
        res?
    };

    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o700))?;

    loop {
        let (stream, _) = listener.accept().await?;
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

        tokio::spawn(handle_connection(
            stream,
            facade.clone(),
            events.clone(),
            shutdown.clone(),
        ));
    }
}

async fn perform_handshake(
    lines: &mut ClientReader,
    writer: &mut ClientWriter,
    facade: &Facade,
) -> io::Result<()> {
    timeout(HANDSHAKE_TIMEOUT, async {
        let first_line = lines.next_line().await?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Client disconnected during handshake",
            )
        })?;

        let request: Request = match serde_json::from_str(&first_line) {
            Ok(req) => req,
            Err(e) => {
                let err_resp = Response {
                    id: serde_json::Value::Null,
                    outcome: Outcome::Err {
                        error: RpcError {
                            code: -32700,
                            message: format!("Parse error in handshake: {e}"),
                        },
                    },
                };
                let _ = write_json(writer, &err_resp).await;
                return Err(io::Error::new(io::ErrorKind::InvalidData, e));
            }
        };

        if request.method != "session.init" {
            let err_resp = Response {
                id: request.id,
                outcome: Outcome::Err {
                    error: RpcError {
                        code: -32004,
                        message: "Expected 'session.init' as first request".to_string(),
                    },
                },
            };
            let _ = write_json(writer, &err_resp).await;
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Missing session.init",
            ));
        }

        match facade.authenticate_session(request.params) {
            Ok(init_data) => {
                let ok_resp = Response {
                    id: request.id,
                    outcome: Outcome::Ok {
                        result: serde_json::to_value(init_data)
                            .expect("Init data always serializes"),
                    },
                };
                write_json(writer, &ok_resp).await?;
                Ok(())
            }
            Err(rpc_err) => {
                let err_resp = Response {
                    id: request.id,
                    outcome: Outcome::Err { error: rpc_err },
                };
                let _ = write_json(writer, &err_resp).await;
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Authentication rejected",
                ))
            }
        }
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Handshake timed out"))?
}

async fn run_session_loop(
    mut lines: ClientReader,
    mut write_half: ClientWriter,
    facade: Arc<Facade>,
    events: FacadeEventBus,
    mut shutdown: sync::watch::Receiver<bool>,
) {
    let mut receiver = events.subscribe();

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("FCM daemon received shutdown signal, exiting...");
                    break;
                }
            }

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
                            error: RpcError {
                                code: -32700,
                                message: format!("parse error: {e}"),
                            },
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

async fn handle_connection(
    stream: UnixStream,
    facade: Arc<Facade>,
    events: FacadeEventBus,
    shutdown: sync::watch::Receiver<bool>,
) {
    let (read_half, mut write_half) = tokio::io::split(stream);
    let limited_reader = read_half.take(MAX_MESSAGE_SIZE_BYTES);
    let mut lines = BufReader::new(limited_reader).lines();

    if let Err(err) = perform_handshake(&mut lines, &mut write_half, &facade).await {
        tracing::warn!(reason = %err, "Handshake failed, dropping peer.");
        return;
    }

    run_session_loop(lines, write_half, facade, events, shutdown).await;
}

async fn write_json<T: serde::Serialize>(
    write_half: &mut WriteHalf<UnixStream>,
    value: &T,
) -> io::Result<()> {
    let mut payload = match serde_json::to_vec(value) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(()),
    };
    payload.push(b'\n');
    write_half.write_all(&payload).await
}
