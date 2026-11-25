//! Caller-facing download types and the `DownloadManager` port.
//!
//! `DownloadRequest` is what callers hand in, `DownloadEvent` is what they
//! observe, and `DownloadManager` is the trait the rest of the application
//! depends on rather than on a concrete manager. The download feature's
//! `DownloadManagerService` implements it.

use crate::domain::download::{Checksum, ChecksumAlgorithm, DownloadError, DownloadSession};
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
    /// The file's size when the caller already knows it. The manager then uses
    /// it as the session's total and skips the HEAD request that would
    /// otherwise size the file, so the start call returns at once.
    pub expected_bytes: Option<u64>,
}

impl DownloadRequest {
    /// A request for one file whose checksum and size are published ahead of
    /// time (the reranker, a vision projector). The SHA-256 is verified once
    /// the bytes are down, and the known size spares the start call a HEAD
    /// request. `display_name` is what the drawer and Activity show: these
    /// files have no model record, so it travels without a model id.
    pub fn pinned(
        url: impl Into<String>,
        destination: impl Into<PathBuf>,
        sha256_hex: &str,
        bytes: u64,
        display_name: impl Into<String>,
    ) -> Result<Self, DownloadError> {
        Ok(Self {
            url: url.into(),
            destination: destination.into(),
            checksum: Some(Checksum::new(
                ChecksumAlgorithm::Sha256,
                sha256_hex.to_string(),
            )?),
            auth_token: None,
            model_name: Some(display_name.into()),
            model_id: None,
            model_file_name: None,
            expected_bytes: Some(bytes),
        })
    }
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

    fn subscribe_to_events(&self) -> Arc<RwLock<Option<mpsc::Receiver<DownloadEvent>>>>;
}

/// True when `raw_url` targets `huggingface.co`, `hf.co`, or a subdomain of either
/// (matched on a dot boundary so lookalike hosts like `evil-huggingface.co` are rejected).
pub fn is_huggingface_url(raw_url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(raw_url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "huggingface.co"
        || host.ends_with(".huggingface.co")
        || host == "hf.co"
        || host.ends_with(".hf.co")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::download::ChecksumAlgorithm;

    #[test]
    fn a_pinned_request_carries_its_checksum_size_and_name_but_no_model() {
        let request = DownloadRequest::pinned(
            "https://example.com/r.gguf",
            "/models/r.gguf",
            &"AB".repeat(32),
            639,
            "Reranker",
        )
        .unwrap();

        let checksum = request.checksum.as_ref().unwrap();
        assert_eq!(checksum.algorithm(), &ChecksumAlgorithm::Sha256);
        assert_eq!(checksum.value(), "ab".repeat(32));
        assert_eq!(request.expected_bytes, Some(639));
        assert_eq!(request.model_name.as_deref(), Some("Reranker"));
        assert_eq!(request.model_id, None);
        assert_eq!(request.model_file_name, None);
        assert_eq!(request.auth_token, None);
    }

    #[test]
    fn a_pinned_request_rejects_a_malformed_checksum() {
        assert!(DownloadRequest::pinned("https://e.com/r", "/m/r", "abc", 1, "R").is_err());
    }

    #[test]
    fn huggingface_url_matcher_respects_dot_boundaries() {
        assert!(is_huggingface_url(
            "https://huggingface.co/org/repo/resolve/main/a.gguf"
        ));
        assert!(is_huggingface_url("https://cdn-lfs.hf.co/file"));
        assert!(is_huggingface_url("https://hf.co/org/repo"));
        assert!(!is_huggingface_url("https://evil-huggingface.co/a.gguf"));
        assert!(!is_huggingface_url("https://example.com/a.gguf"));
    }
}
