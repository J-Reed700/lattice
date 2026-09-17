//! Caller-facing download types and the `DownloadManager` port.
//!
//! `DownloadRequest` is what callers hand in, `DownloadEvent` is what they
//! observe, and `DownloadManager` is the trait the rest of the application
//! depends on rather than on a concrete manager.

use crate::domain::download::{Checksum, DownloadError, DownloadSession};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub destination: PathBuf,
    pub checksum: Option<Checksum>,
    pub auth_token: Option<String>,
    pub model_name: Option<String>,
    pub model_id: Option<String>,
    /// Manifest-relative identity stored in `model_files.file_name`.
    /// This may contain a safe subdirectory such as `onnx/model.onnx` and
    /// must not be re-derived from the destination basename.
    pub model_file_name: Option<String>,
}

/// One file in a model batch, with URLs ordered from preferred to fallback.
pub struct DownloadBatchItem {
    pub requests: Vec<DownloadRequest>,
}

#[derive(Clone)]
pub enum DownloadEvent {
    Started {
        id: String,
    },
    Progress {
        id: String,
        bytes_downloaded: u64,
        bytes_per_second: f64,
    },
    Paused {
        id: String,
    },
    Resumed {
        id: String,
    },
    Completed {
        id: String,
    },
    Failed {
        id: String,
        error: String,
    },
    Cancelled {
        id: String,
    },
}

#[async_trait]
pub trait DownloadManager: Send + Sync {
    async fn start_download(&self, request: DownloadRequest) -> Result<String, DownloadError>;

    /// Register one request without promoting the queue. Batch orchestration
    /// uses this primitive while holding the manager's queue gate.
    #[doc(hidden)]
    async fn enqueue_download(&self, request: DownloadRequest) -> Result<String, DownloadError>;

    /// Register every file before releasing the batch to the worker queue.
    async fn start_download_batch(
        &self,
        items: Vec<DownloadBatchItem>,
    ) -> Result<Vec<String>, DownloadError>;

    async fn pause_download(&self, id: &str) -> Result<(), DownloadError>;

    async fn resume_download(&self, id: &str) -> Result<(), DownloadError>;

    async fn cancel_download(&self, id: &str) -> Result<(), DownloadError>;

    async fn get_download_status(&self, id: &str)
        -> Result<Option<DownloadSession>, DownloadError>;

    async fn list_downloads(&self) -> Result<Vec<DownloadSession>, DownloadError>;

    async fn list_active_downloads(&self) -> Result<Vec<DownloadSession>, DownloadError>;

    async fn retry_download(&self, id: &str) -> Result<(), DownloadError>;

    async fn delete_download(&self, id: &str) -> Result<(), DownloadError>;

    async fn clear_completed_downloads(&self) -> Result<usize, DownloadError>;

    /// Delete all pending downloads for a specific model
    async fn delete_pending_by_model(&self, model_id: &str) -> Result<u64, DownloadError>;

    /// Process pending queue items if concurrency slots are available.
    async fn process_pending_queue(&self) -> Result<(), DownloadError>;

    fn subscribe_to_events(&self) -> Arc<RwLock<Option<mpsc::UnboundedReceiver<DownloadEvent>>>>;
}
