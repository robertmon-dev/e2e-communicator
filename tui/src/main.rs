use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use ratatui::crossterm::event::{Event as TerminalEvent, EventStream};
use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt;

use tui::app::state::{App, StreamKind};
use tui::app::update::{Event, update};
use tui::config::{resolve_socket_path, resolve_token};
use tui::i18n::{self, Locale, t, tf};
use tui::ipc::connection::{self, Incoming};
use tui::ipc::protocol::Request;

const TICK: Duration = Duration::from_millis(250);
const QUEUE_DEPTH: usize = 256;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Parser)]
#[command(about = "Terminal UI for the e2e-communicator client")]
struct Args {
    #[arg(long, env = "APP__FACADE__SOCKET_PATH")]
    socket: Option<PathBuf>,

    #[arg(long, env = "APP__FACADE__SESSION_TOKEN")]
    token: Option<String>,

    #[arg(long)]
    lang: Option<String>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    i18n::set_locale(
        args.lang
            .as_deref()
            .and_then(Locale::parse)
            .unwrap_or_else(Locale::from_env),
    );
    let socket_path = resolve_socket_path(args.socket, None);
    let token = resolve_token(args.token, None);

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        previous_hook(info);
    }));

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, socket_path, token).await;
    ratatui::restore();

    if let Ok(Shutdown::TimedOut) = &result {
        eprintln!(
            "{}",
            tf(
                "app.shutdown_timed_out",
                &[("grace", &format!("{SHUTDOWN_GRACE:?}"))]
            )
        );
    }

    result.map(|_| ())
}

#[derive(Debug, PartialEq, Eq)]
enum Shutdown {
    Clean,
    TimedOut,
}

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    socket_path: PathBuf,
    token: Option<String>,
) -> std::io::Result<Shutdown> {
    let (event_tx, mut event_rx) = mpsc::channel::<Event>(QUEUE_DEPTH);
    let (request_tx, request_rx) = mpsc::channel::<Request>(QUEUE_DEPTH);

    let (incoming_tx, mut incoming_rx) = mpsc::channel::<Incoming>(QUEUE_DEPTH);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let connection = tokio::spawn(connection::run(
        socket_path,
        token,
        incoming_tx,
        request_rx,
        shutdown_rx,
    ));

    let socket_events = event_tx.clone();
    tokio::spawn(async move {
        while let Some(incoming) = incoming_rx.recv().await {
            let event = match incoming {
                Incoming::Connected => Event::Connected,
                Incoming::Disconnected { reason } => Event::Disconnected { reason },
                Incoming::Response { id, outcome } => Event::Response { id, outcome },
                Incoming::Notification { method, params } => Event::Notification { method, params },
                Incoming::Malformed { line } => Event::Malformed { line },
            };

            if socket_events.send(event).await.is_err() {
                return;
            }
        }
    });

    let key_events = event_tx.clone();
    let keys = tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(Ok(terminal_event)) = reader.next().await {
            if let TerminalEvent::Key(key) = terminal_event
                && key.is_press()
                && key_events.send(Event::Key(key)).await.is_err()
            {
                return;
            }
        }
    });

    let tick_events = event_tx.clone();
    let ticks = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        loop {
            ticker.tick().await;
            if tick_events.send(Event::Tick).await.is_err() {
                return;
            }
        }
    });

    let signal_events = event_tx;
    let signals = tokio::spawn(async move {
        let mut terminate =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(_) => return,
            };

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }

        let _ = signal_events.send(Event::Quit).await;
    });

    let mut app = App::new();
    terminal.draw(|frame| tui::ui::draw(frame, &app))?;

    while let Some(event) = event_rx.recv().await {
        for request in update(&mut app, event) {
            if request_tx.try_send(request).is_err() {
                app.log(StreamKind::Problem, t("app.queue_full"));
            }
        }

        if app.should_quit {
            break;
        }

        terminal.draw(|frame| tui::ui::draw(frame, &app))?;
    }

    app.log(StreamKind::Info, t("app.shutting_down"));
    terminal.draw(|frame| tui::ui::draw(frame, &app))?;

    keys.abort();
    ticks.abort();
    signals.abort();

    let _ = shutdown_tx.send(true);
    drop(request_tx);

    match tokio::time::timeout(SHUTDOWN_GRACE, connection).await {
        Ok(_) => Ok(Shutdown::Clean),
        Err(_) => Ok(Shutdown::TimedOut),
    }
}
