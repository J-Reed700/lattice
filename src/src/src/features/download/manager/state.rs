//! Session and state bookkeeping for the download manager.
//!
//! Owns the manager's mutable state — the active-download table, the pending
//! queue, per-session auth tokens and the event channel — plus the stop
//! signal that running transfer tasks listen on.

use super::types::DownloadEvent;
use crate::domain::download::DownloadSession;
use crate::features::download::download_repository::DownloadRepository;
use crate::features::download::engine::DownloadEngine;
use crate::infrastructure::services::file_cleanup::FileCleanupService;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

/// Why a running download task is being told to stop.
///
/// Pause and cancel previously shared one signal carrying no payload, so the
/// task could only assume the terminal case: it ran `session.cancel()` on
/// every stop. Because `Paused → Cancelled` is a permitted transition, pausing
/// a download drove the row to `Cancelled` — and `resume_download` only accepts
/// `Paused | Failed`, so resume then refused forever. The one button that still
/// worked, retry, deletes the partial file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StopReason {
    /// Stop transferring but keep the partial file and leave the row in
    /// `Paused` — `pause_download` has already written that state.
    Pause,
    /// Terminal stop: transition the session to `Cancelled`.
    Cancel,
}

pub(super) struct ActiveDownload {
    // reason: the registry owns the in-flight session snapshot for the lifetime of the transfer.
    #[allow(dead_code)]
    pub(super) session: DownloadSession,
    // reason: keeps the spawned transfer task's join handle alive while the download is active.
    #[allow(dead_code)]
    pub(super) task_handle: Option<JoinHandle<()>>,
    pub(super) cancel_tx: Option<mpsc::Sender<StopReason>>,
}

pub struct DownloadManagerService {
    pub(super) repository: Arc<dyn DownloadRepository>,
    pub(super) engine: Arc<dyn DownloadEngine>,
    pub(super) active_downloads: Arc<RwLock<HashMap<String, ActiveDownload>>>,
    pub(super) download_queue: Arc<RwLock<VecDeque<String>>>,
    pub(super) max_concurrent_downloads: usize,
    pub(super) event_tx: mpsc::UnboundedSender<DownloadEvent>,
    pub(super) event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<DownloadEvent>>>>,
    pub(super) auth_tokens: Arc<RwLock<HashMap<String, String>>>,
    pub(super) file_cleanup: Arc<FileCleanupService>,
    pub(super) allowed_root: PathBuf,
}

impl DownloadManagerService {
    pub fn new(
        repository: Arc<dyn DownloadRepository>,
        engine: Arc<dyn DownloadEngine>,
        allowed_root: PathBuf,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            repository,
            engine,
            active_downloads: Arc::new(RwLock::new(HashMap::new())),
            download_queue: Arc::new(RwLock::new(VecDeque::new())),
            max_concurrent_downloads: 2,
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
            auth_tokens: Arc::new(RwLock::new(HashMap::new())),
            file_cleanup: Arc::new(FileCleanupService::new()),
            allowed_root,
        }
    }

    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent_downloads = max;
        self
    }
}
