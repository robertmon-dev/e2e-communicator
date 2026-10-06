use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::domain::state::connectivity::ConnectivityState;

const DEFAULT_PROBE_TARGETS: &[(&str, u16)] = &[("1.1.1.1", 443), ("8.8.8.8", 443)];
const DEFAULT_INTERVAL: Duration = Duration::from_secs(15);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_FAILURE_THRESHOLD: u32 = 3;

pub struct ConnectivityMonitor {
    connectivity: Arc<ConnectivityState>,
    probe_targets: Vec<(String, u16)>,
    interval: Duration,
    timeout: Duration,
    failure_threshold: u32,
}

impl ConnectivityMonitor {
    pub fn new(connectivity: Arc<ConnectivityState>) -> Self {
        Self {
            connectivity,
            probe_targets: DEFAULT_PROBE_TARGETS
                .iter()
                .map(|(host, port)| (host.to_string(), *port))
                .collect(),
            interval: DEFAULT_INTERVAL,
            timeout: DEFAULT_TIMEOUT,
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
        }
    }

    pub fn with_targets(mut self, probe_targets: Vec<(String, u16)>) -> Self {
        self.probe_targets = probe_targets;
        self
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_failure_threshold(mut self, failure_threshold: u32) -> Self {
        self.failure_threshold = failure_threshold;
        self
    }

    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) {
        if *shutdown.borrow_and_update() {
            return;
        }

        let mut consecutive_failures: u32 = 0;

        loop {
            let is_online = tokio::select! {
                biased;

                _ = shutdown.wait_for(|&down| down) => {
                    tracing::info!("Connectivity check stopped by shutdown signal");
                    break;
                }

                online = self.probe_once() => online,
            };

            if is_online {
                consecutive_failures = 0;
                self.connectivity.set_online();
            } else {
                consecutive_failures = consecutive_failures.saturating_add(1);

                if consecutive_failures >= self.failure_threshold {
                    self.connectivity.set_offline();
                }
            }

            let delay = common::backoff::with_jitter(self.interval);

            tokio::select! {
                biased;

                _ = shutdown.wait_for(|&down| down) => {
                    tracing::info!("Health check sleep interrupted by shutdown signal");
                    break;
                }

                _ = tokio::time::sleep(delay) => {}
            }
        }
    }

    async fn probe_once(&self) -> bool {
        let mut set = tokio::task::JoinSet::new();

        for (host, port) in &self.probe_targets {
            let addr = format!("{host}:{port}");
            let to = self.timeout;

            set.spawn(async move {
                match timeout(to, TcpStream::connect(&addr)).await {
                    Ok(Ok(_stream)) => true,
                    _ => false,
                }
            });
        }

        while let Some(res) = set.join_next().await {
            if let Ok(true) = res {
                set.abort_all();
                return true;
            }
        }

        false
    }
}
