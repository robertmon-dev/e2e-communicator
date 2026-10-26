use std::error::Error as StdError;
use std::io;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinSet;
use tokio::time::{Duration, timeout};

use super::gate::Gate;
use super::protocol::{Notification, Outcome, Request, Response, RpcError};
use crate::domain::state::facade_events::FacadeEventBus;

pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_MESSAGE_SIZE_BYTES: usize = 2 * 1024 * 1024;

const SOCKET_FILE_NAME: &str = "e2e-communicator-client.sock";
const HANDSHAKE_METHOD: &str = "session.init";

mod error_code {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INTERNAL_ERROR: i32 = -32603;
    pub const HANDSHAKE_REQUIRED: i32 = -32004;
}

struct SocketGuard<'a>(&'a Path);

impl Drop for SocketGuard<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0);
    }
}

pub fn default_socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(SOCKET_FILE_NAME)
}

fn remove_stale_socket(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_socket() => std::fs::remove_file(path),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} exists and is not a socket", path.display()),
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn bind_private_socket(path: &Path) -> io::Result<(UnixListener, SocketGuard<'_>)> {
    remove_stale_socket(path)?;

    let prev_mask = unsafe { libc::umask(0o077) };
    let bound = UnixListener::bind(path);
    unsafe { libc::umask(prev_mask) };

    let listener = bound?;
    let guard = SocketGuard(path);
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;

    Ok((listener, guard))
}

fn is_peer_authorized(stream: &UnixStream, own_uid: libc::uid_t) -> bool {
    match stream.peer_cred() {
        Ok(cred) if cred.uid() == own_uid => true,
        Ok(cred) => {
            tracing::warn!(
                peer_uid = cred.uid(),
                expected_uid = own_uid,
                "Unauthorized connection attempt from different user, dropping."
            );
            false
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to get peer credentials, dropping connection.");
            false
        }
    }
}

async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|&down| down).await;
}

pub async fn run(
    socket_path: &Path,
    gate: Arc<Gate>,
    events: FacadeEventBus,
    mut shutdown: watch::Receiver<bool>,
) -> io::Result<()> {
    let (listener, socket_guard) = bind_private_socket(socket_path)?;
    let own_uid = rustix::process::getuid().as_raw();
    let mut sessions = JoinSet::new();

    tracing::info!("IPC facade server started");

    loop {
        let stream = tokio::select! {
            biased;

            _ = shutdown_requested(&mut shutdown) => {
                tracing::info!("IPC listener received shutdown signal, stopping accept loop...");
                break;
            }

            Some(joined) = sessions.join_next(), if !sessions.is_empty() => {
                match joined {
                    Err(e) if e.is_panic() => tracing::error!(error = %e, "IPC session task panicked"),
                    _ => {}
                }
                continue;
            }

            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to accept IPC connection");
                    continue;
                }
            },
        };

        if !is_peer_authorized(&stream, own_uid) {
            continue;
        }

        sessions.spawn(handle_connection(
            stream,
            gate.clone(),
            events.clone(),
            shutdown.clone(),
        ));
    }

    drop(listener);
    drop(socket_guard);

    tracing::info!(
        active_clients = sessions.len(),
        "Waiting for active IPC sessions to finish..."
    );
    while sessions.join_next().await.is_some() {}

    tracing::info!("IPC facade cleanly stopped.");
    Ok(())
}

async fn handle_connection(
    stream: UnixStream,
    gate: Arc<Gate>,
    events: FacadeEventBus,
    mut shutdown: watch::Receiver<bool>,
) {
    let (read_half, mut writer) = stream.into_split();
    let mut reader = FrameReader::new(read_half);

    let handshake = tokio::select! {
        biased;
        _ = shutdown_requested(&mut shutdown) => return,
        res = perform_handshake(&mut reader, &mut writer, &gate) => res,
    };

    if let Err(err) = handshake {
        tracing::warn!(reason = %err, "Handshake failed, dropping peer.");
        return;
    }

    run_session_loop(reader, writer, &gate, events, shutdown).await;
}

async fn perform_handshake(
    reader: &mut FrameReader,
    writer: &mut OwnedWriteHalf,
    gate: &Gate,
) -> io::Result<()> {
    let handshake = async {
        let frame = reader.next_frame().await?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Client disconnected during handshake",
            )
        })?;

        let request: Request = match serde_json::from_slice(&frame) {
            Ok(request) => request,
            Err(e) => {
                let response = error_response(
                    Value::Null,
                    error_code::PARSE_ERROR,
                    format!("Parse error in handshake: {e}"),
                );
                return reject(writer, &response, io::ErrorKind::InvalidData, e).await;
            }
        };

        if request.method != HANDSHAKE_METHOD {
            let response = error_response(
                request.id,
                error_code::HANDSHAKE_REQUIRED,
                format!("Expected '{HANDSHAKE_METHOD}' as first request"),
            );
            return reject(
                writer,
                &response,
                io::ErrorKind::PermissionDenied,
                "Missing session.init",
            )
            .await;
        }

        let init_data = match gate.authenticate_session(request.params).await {
            Ok(init_data) => init_data,
            Err(error) => {
                let response = Response {
                    id: request.id,
                    outcome: Outcome::Err { error },
                };
                return reject(
                    writer,
                    &response,
                    io::ErrorKind::PermissionDenied,
                    "Authentication rejected",
                )
                .await;
            }
        };

        let result = match serde_json::to_value(init_data) {
            Ok(result) => result,
            Err(e) => {
                let response =
                    error_response(request.id, error_code::INTERNAL_ERROR, "Internal error");
                return reject(writer, &response, io::ErrorKind::InvalidData, e).await;
            }
        };

        let response = Response {
            id: request.id,
            outcome: Outcome::Ok { result },
        };
        write_json(writer, &response).await
    };

    timeout(HANDSHAKE_TIMEOUT, handshake)
        .await
        .unwrap_or_else(|_| {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Handshake timed out",
            ))
        })
}

async fn run_session_loop(
    mut reader: FrameReader,
    mut writer: OwnedWriteHalf,
    gate: &Gate,
    events: FacadeEventBus,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut notifications = events.subscribe();

    loop {
        tokio::select! {
            biased;

            _ = shutdown_requested(&mut shutdown) => {
                tracing::info!("Session loop received shutdown signal, closing connection...");
                break;
            }

            frame = reader.next_frame() => {
                let frame = match frame {
                    Ok(Some(frame)) => frame,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to read client message, dropping connection");
                        break;
                    }
                };

                let response = match serde_json::from_slice::<Request>(&frame) {
                    Ok(request) => gate.handle(request).await,
                    Err(e) => error_response(
                        Value::Null,
                        error_code::PARSE_ERROR,
                        format!("parse error: {e}"),
                    ),
                };

                if write_json(&mut writer, &response).await.is_err() {
                    break;
                }
            }

            event = notifications.recv() => match event {
                Ok(event) => {
                    let notification: Notification = event.into();
                    if write_json(&mut writer, &notification).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "Facade event forwarder lagged, events were skipped");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
}

struct FrameReader {
    inner: BufReader<OwnedReadHalf>,
    buf: Vec<u8>,
}

impl FrameReader {
    fn new(read_half: OwnedReadHalf) -> Self {
        Self {
            inner: BufReader::new(read_half),
            buf: Vec::new(),
        }
    }

    async fn next_frame(&mut self) -> io::Result<Option<Vec<u8>>> {
        loop {
            let budget = (MAX_MESSAGE_SIZE_BYTES + 1).saturating_sub(self.buf.len());
            if budget == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Payload too large",
                ));
            }

            let read = (&mut self.inner)
                .take(budget as u64)
                .read_until(b'\n', &mut self.buf)
                .await?;

            if self.buf.last() == Some(&b'\n') {
                let mut frame = std::mem::take(&mut self.buf);
                frame.pop();
                if frame.last() == Some(&b'\r') {
                    frame.pop();
                }
                return Ok(Some(frame));
            }

            if read == 0 {
                let rest = std::mem::take(&mut self.buf);
                return Ok((!rest.is_empty()).then_some(rest));
            }
        }
    }
}

fn error_response(id: Value, code: i32, message: impl Into<String>) -> Response {
    Response {
        id,
        outcome: Outcome::Err {
            error: RpcError {
                code,
                message: message.into(),
            },
        },
    }
}

async fn reject<E>(
    writer: &mut OwnedWriteHalf,
    response: &Response,
    kind: io::ErrorKind,
    error: E,
) -> io::Result<()>
where
    E: Into<Box<dyn StdError + Send + Sync>>,
{
    let _ = write_json(writer, response).await;
    Err(io::Error::new(kind, error))
}

async fn write_json<W, T>(writer: &mut W, value: &T) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let mut payload = serde_json::to_vec(value).map_err(|e| {
        tracing::error!(error = %e, "Failed to serialize outgoing IPC message");
        io::Error::new(io::ErrorKind::InvalidData, e)
    })?;
    payload.push(b'\n');

    timeout(WRITE_TIMEOUT, writer.write_all(&payload))
        .await
        .unwrap_or_else(|_| Err(io::Error::new(io::ErrorKind::TimedOut, "Write timed out")))
}
