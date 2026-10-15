//! The OCR server's lifecycle: started on demand, one page at a time, stopped
//! after [`OCR_IDLE_TTL`] without a page and when the app shuts down.
//!
//! The OCR model and its projector take about 2 GB of RAM while loaded, on
//! machines that may also be holding a chat model. So the process exists only
//! while scans are being read: the first page starts it (under a second from
//! a warm disk), each page refreshes its idle clock, and a reaper task stops it
//! once the clock runs out. The reaper also listens for app shutdown, which is
//! the only way to kill the child before the runtime is torn down — the
//! shutdown path never drops this struct.

use std::future::Future;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex as SyncMutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::model::OcrModel;
use lattice_core::application::ports::OcrError;
use lattice_core::shared::runtime::background::BackgroundTasks;

/// How long the server stays up after its last page. Reloading costs a second
/// or two; keeping 2 GB resident between unrelated imports costs more.
pub const OCR_IDLE_TTL: Duration = Duration::from_secs(60);

/// The OS process behind a server.
pub trait ServerProcess: Send {
    /// Whether the process has exited (crashed, or was killed).
    fn has_exited(&mut self) -> bool;
    /// Kill the process now. Must be idempotent.
    fn kill(&mut self);
}

type SharedProcess = Arc<SyncMutex<Box<dyn ServerProcess>>>;

/// A started, ready server.
pub struct RunningServer {
    address: ServerAddress,
    process: SharedProcess,
}

impl RunningServer {
    pub fn new(endpoint: String, api_key: String, process: Box<dyn ServerProcess>) -> Self {
        Self {
            address: ServerAddress { endpoint, api_key },
            process: Arc::new(SyncMutex::new(process)),
        }
    }

    fn has_exited(&self) -> bool {
        self.process.lock().has_exited()
    }
}

impl Drop for RunningServer {
    /// Whoever lets go of the server — idle reaper, restart, shutdown — kills
    /// it; nothing may leave a 2 GB process behind.
    fn drop(&mut self) {
        self.process.lock().kill();
    }
}

/// What a page request needs to reach the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAddress {
    /// Base URL, e.g. `http://127.0.0.1:52344`.
    pub endpoint: String,
    /// Bearer token the server was started with.
    pub api_key: String,
}

/// Starts OCR servers.
#[async_trait]
pub trait ServerLauncher: Send + Sync {
    /// Start a server for `model` and return once it answers `/health`.
    ///
    /// Fails with [`OcrError::Unavailable`]: if the server cannot start, no
    /// page can be read.
    async fn launch(&self, model: &OcrModel) -> Result<RunningServer, OcrError>;
}

struct Slot {
    server: RunningServer,
    model: OcrModel,
}

struct Inner {
    launcher: Arc<dyn ServerLauncher>,
    idle_ttl: Duration,
    /// Held for the whole of a page. That is the concurrency bound (one page
    /// per process; a vision prompt fills the single slot's context anyway)
    /// and it is what keeps the reaper from stopping a server mid-page.
    slot: tokio::sync::Mutex<Option<Slot>>,
    last_used: SyncMutex<Instant>,
    /// The app's background scope: the reaper runs in it, and its
    /// cancellation is the app shutting down.
    background: Arc<BackgroundTasks>,
}

/// The OCR server, as a lease-per-page resource.
pub struct OcrServer {
    inner: Arc<Inner>,
}

impl OcrServer {
    pub fn new(launcher: Arc<dyn ServerLauncher>, background: Arc<BackgroundTasks>) -> Self {
        Self::with_idle_ttl(launcher, OCR_IDLE_TTL, background)
    }

    pub fn with_idle_ttl(
        launcher: Arc<dyn ServerLauncher>,
        idle_ttl: Duration,
        background: Arc<BackgroundTasks>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                launcher,
                idle_ttl,
                slot: tokio::sync::Mutex::new(None),
                last_used: SyncMutex::new(Instant::now()),
                background,
            }),
        }
    }

    /// Run `page` against a ready server for `model`, starting one if none is
    /// running, or replacing one that crashed or serves another model.
    pub async fn run<T, F, Fut>(&self, model: &OcrModel, page: F) -> Result<T, OcrError>
    where
        F: FnOnce(ServerAddress) -> Fut,
        Fut: Future<Output = Result<T, OcrError>>,
    {
        let mut slot = self.inner.slot.lock().await;
        if slot
            .as_ref()
            .is_some_and(|running| running.model != *model || running.server.has_exited())
        {
            warn!("OCR server exited or serves another model; starting a new one");
            *slot = None;
        }
        let address = match slot.as_ref() {
            Some(running) => running.server.address.clone(),
            None => {
                let started = Instant::now();
                let server = self.inner.launcher.launch(model).await?;
                let address = server.address.clone();
                self.supervise(&server)?;
                info!(
                    model_id = %model.id,
                    endpoint = %address.endpoint,
                    load_ms = started.elapsed().as_millis() as u64,
                    "OCR server started"
                );
                *slot = Some(Slot {
                    server,
                    model: model.clone(),
                });
                address
            }
        };
        *self.inner.last_used.lock() = Instant::now();
        let result = page(address).await;
        *self.inner.last_used.lock() = Instant::now();
        if slot
            .as_ref()
            .is_some_and(|running| running.server.has_exited())
        {
            // Replaced on the next page rather than failing every one after it.
            warn!("OCR server exited while reading a page");
            *slot = None;
        }
        result
    }

    /// Stop the server now, if one is running.
    pub async fn stop(&self) {
        self.inner.slot.lock().await.take();
    }

    /// Whether a server is running.
    pub async fn is_running(&self) -> bool {
        self.inner.slot.lock().await.is_some()
    }

    /// Start the reaper for a server that just came up.
    fn supervise(&self, server: &RunningServer) -> Result<(), OcrError> {
        let reaper = reap(
            Arc::downgrade(&self.inner),
            Arc::clone(&server.process),
            self.inner.background.token(),
        );
        if self.inner.background.spawn(reaper).is_none() {
            // Admission is closed: the app is shutting down. A server nobody
            // will stop must not be handed out; the caller drops it, which
            // kills it.
            return Err(OcrError::Unavailable {
                reason: "the app is shutting down".into(),
            });
        }
        Ok(())
    }
}

fn idle_expired(last_used: Instant, now: Instant, idle_ttl: Duration) -> bool {
    now.saturating_duration_since(last_used) >= idle_ttl
}

/// Stop `process` once its server has been idle for the TTL, or at shutdown.
///
/// Returns as soon as the slot no longer holds this process (restarted or
/// stopped by someone else), so each server has exactly one reaper.
async fn reap(server: Weak<Inner>, process: SharedProcess, shutdown: CancellationToken) {
    loop {
        let Some(inner) = server.upgrade() else {
            process.lock().kill();
            return;
        };
        let deadline = *inner.last_used.lock() + inner.idle_ttl;
        drop(inner);
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => {
                // Kill first, without the slot: a page in flight would hold
                // it past the app's shutdown budget.
                process.lock().kill();
                return;
            }
            _ = tokio::time::sleep_until(deadline.into()) => {}
        }
        let Some(inner) = server.upgrade() else {
            process.lock().kill();
            return;
        };
        let mut slot = tokio::select! {
            biased;
            _ = shutdown.cancelled() => {
                process.lock().kill();
                return;
            }
            slot = inner.slot.lock() => slot,
        };
        let ours = slot
            .as_ref()
            .is_some_and(|running| Arc::ptr_eq(&running.server.process, &process));
        if !ours {
            return;
        }
        if idle_expired(*inner.last_used.lock(), Instant::now(), inner.idle_ttl) {
            *slot = None;
            info!("OCR server stopped after being idle");
            return;
        }
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
