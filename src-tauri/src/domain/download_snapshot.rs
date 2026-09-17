use serde::{Deserialize, Serialize};

/// File-level download progress
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSnapshot {
    /// The real DownloadSession id used by pause/resume/cancel operations.
    pub id: String,
    pub filename: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub status: FileStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Pending,
    Downloading,
    Completed,
    Error,
}

/// Single-file download snapshot
///
/// Represents simple downloads with one file (PDFs, images, documents).
/// Architecturally distinct from Batch which handles multi-file model downloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleFileSnapshot {
    pub id: String,
    pub filename: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: u64,
    pub percentage: Option<f64>,
    pub eta_seconds: Option<u64>,
    pub status: DownloadStatus,
}

/// Multi-file download batch snapshot
///
/// Represents complex downloads with multiple files (LLM models with tokenizer, config, weights).
/// Provides aggregate metrics across all files in the batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSnapshot {
    pub id: String,
    pub group_name: String,
    pub files: Vec<FileSnapshot>,
    pub total_files: u32,
    pub completed_files: u32,
    pub aggregate_bytes_downloaded: u64,
    pub aggregate_total_bytes: u64,
    pub aggregate_bytes_per_second: u64,
    pub aggregate_percentage: f64,
    pub aggregate_eta_seconds: Option<u64>,
    pub status: DownloadStatus,
}

/// Discriminated union of download snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DownloadStateSnapshot {
    #[serde(rename = "single")]
    Single(SingleFileSnapshot),
    #[serde(rename = "batch")]
    Batch(BatchSnapshot),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Error,
    Cancelled,
}
