use std::io;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedReadHalf;
use tokio::sync::{mpsc, watch};

use crate::i18n::{t, tf};
use crate::ipc::protocol::{
    Frame, Outcome, PROTOCOL_VERSION, Request, SESSION_INIT_METHOD, parse_frame,
};

pub const HANDSHAKE_ID: u64 = u64::MAX;

const BACKOFF_START: Duration = Duration::from_millis(500);
const BACKOFF_MAX: Duration = Duration::from_secs(5);
const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
const MALFORMED_EXCERPT_BYTES: usize = 400;

#[derive(Debug)]
pub enum Incoming {
    Connected,
    Disconnected { reason: String },
    Response { id: u64, outcome: Outcome },
    Notification { method: String, params: Value },
    Malformed { line: String },
}

pub async fn run(
    socket_path: PathBuf,
    token: Option<String>,
    events: mpsc::Sender<Incoming>,
    mut requests: mpsc::Receiver<Request>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut backoff = BACKOFF_START;

    while !*shutdown.borrow_and_update() {
        match UnixStream::connect(&socket_path).await {
            Ok(stream) => {
                let started = tokio::time::Instant::now();

                if events.send(Incoming::Connected).await.is_err() {
                    return;
                }

                let reason = serve(stream, &token, &events, &mut requests, &mut shutdown).await;

                if is_set(&shutdown) {
                    return;
                }

                if events
                    .send(Incoming::Disconnected { reason })
                    .await
                    .is_err()
                {
                    return;
                }

                if started.elapsed() >= BACKOFF_MAX {
                    backoff = BACKOFF_START;
                } else if !sleep_unless_shutdown(backoff, &mut shutdown).await {
                    return;
                } else {
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                }
            }
            Err(err) => {
                if events
                    .send(Incoming::Disconnected {
                        reason: err.to_string(),
                    })
                    .await
                    .is_err()
                {
                    return;
                }

                if !sleep_unless_shutdown(backoff, &mut shutdown).await {
                    return;
                }
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
}

fn is_set(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow()
}

async fn sleep_unless_shutdown(delay: Duration, shutdown: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => true,
        _ = shutdown.changed() => false,
    }
}

async fn serve(
    stream: UnixStream,
    token: &Option<String>,
    events: &mpsc::Sender<Incoming>,
    requests: &mut mpsc::Receiver<Request>,
    shutdown: &mut watch::Receiver<bool>,
) -> String {
    let (read_half, mut write_half) = stream.into_split();
    let mut frames = FrameReader::new(read_half);

    let handshake = Request {
        id: HANDSHAKE_ID,
        method: SESSION_INIT_METHOD.to_string(),
        params: match token {
            Some(token) => {
                serde_json::json!({ "protocol_version": PROTOCOL_VERSION, "token": token })
            }
            None => serde_json::json!({ "protocol_version": PROTOCOL_VERSION }),
        },
    };

    if let Err(err) = write_line(&mut write_half, &handshake).await {
        return err;
    }

    loop {
        tokio::select! {
            line = frames.next_line() => match line {
                Ok(Some(line)) => {
                    let event = match parse_frame(&line) {
                        Ok(Frame::Response(response)) => {
                            match response.id.as_u64() {
                                Some(id) => Incoming::Response { id, outcome: response.outcome },
                                None => Incoming::Malformed { line },
                            }
                        }
                        Ok(Frame::Notification(note)) => Incoming::Notification {
                            method: note.method,
                            params: note.params,
                        },
                        Err(_) => Incoming::Malformed { line },
                    };

                    if events.send(event).await.is_err() {
                        return t("conn.app_stopped_listening");
                    }
                }
                Ok(None) => return t("conn.closed_by_client"),
                Err(FrameError::Oversized { excerpt }) => {
                    let _ = events.send(Incoming::Malformed { line: excerpt }).await;
                    return tf(
                        "conn.frame_too_large",
                        &[("limit", &MAX_FRAME_BYTES.to_string())],
                    );
                }
                Err(FrameError::Io(err)) => return err.to_string(),
            },

            request = requests.recv() => match request {
                Some(request) => {
                    if let Err(err) = write_line(&mut write_half, &request).await {
                        return err;
                    }
                }
                None => {
                    let _ = write_half.shutdown().await;
                    return t("conn.app_stopped_sending");
                }
            },

            _ = shutdown.changed() => {
                let reason = match flush(&mut write_half, requests).await {
                    Ok(()) => t("conn.shutting_down"),
                    Err(err) => err,
                };
                let _ = write_half.shutdown().await;
                return reason;
            }
        }
    }
}

async fn flush<W>(writer: &mut W, requests: &mut mpsc::Receiver<Request>) -> Result<(), String>
where
    W: AsyncWriteExt + Unpin,
{
    while let Ok(request) = requests.try_recv() {
        write_line(writer, &request).await?;
    }

    writer.flush().await.map_err(|err| err.to_string())
}

async fn write_line<W>(writer: &mut W, request: &Request) -> Result<(), String>
where
    W: AsyncWriteExt + Unpin,
{
    let mut payload = serde_json::to_vec(request).map_err(|err| err.to_string())?;
    payload.push(b'\n');
    writer
        .write_all(&payload)
        .await
        .map_err(|err| err.to_string())
}

#[derive(Debug)]
enum FrameError {
    Oversized { excerpt: String },
    Io(io::Error),
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

    async fn next_line(&mut self) -> Result<Option<String>, FrameError> {
        loop {
            let budget = (MAX_FRAME_BYTES + 1).saturating_sub(self.buf.len());
            if budget == 0 {
                return Err(FrameError::Oversized {
                    excerpt: excerpt(&self.buf),
                });
            }

            let read = (&mut self.inner)
                .take(budget as u64)
                .read_until(b'\n', &mut self.buf)
                .await
                .map_err(FrameError::Io)?;

            if self.buf.last() == Some(&b'\n') {
                let mut frame = std::mem::take(&mut self.buf);
                frame.pop();
                if frame.last() == Some(&b'\r') {
                    frame.pop();
                }
                return Ok(Some(String::from_utf8_lossy(&frame).into_owned()));
            }

            if read == 0 {
                let rest = std::mem::take(&mut self.buf);
                return Ok((!rest.is_empty()).then(|| String::from_utf8_lossy(&rest).into_owned()));
            }
        }
    }
}

fn excerpt(bytes: &[u8]) -> String {
    let end = bytes.len().min(MALFORMED_EXCERPT_BYTES);
    let mut text = String::from_utf8_lossy(&bytes[..end]).into_owned();

    if bytes.len() > end {
        text.push_str(&tf(
            "conn.excerpt_more",
            &[("bytes", &(bytes.len() - end).to_string())],
        ));
    }

    text
}
