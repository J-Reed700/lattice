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
    Paused,
    Completed,
    Error,
    Cancelled,
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
    /// The full final path, the same string the download list reports.
    pub destination: String,
    /// Display name, when the download has one (with or without a model id).
    pub model_name: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_file_snapshot_carries_its_destination_and_model_name() {
        let snapshot = SingleFileSnapshot {
            id: "d".into(),
            filename: "r.gguf".into(),
            destination: "/models/r.gguf".into(),
            model_name: Some("Reranker".into()),
            bytes_downloaded: 1,
            total_bytes: Some(2),
            bytes_per_second: 0,
            percentage: Some(50.0),
            eta_seconds: None,
            status: DownloadStatus::Pending,
        };
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["destination"], "/models/r.gguf");
        assert_eq!(json["modelName"], "Reranker");
    }
}

#[cfg(test)]
mod file_status_tests {
    use super::*;

    #[test]
    fn paused_and_cancelled_files_serialize_like_the_other_statuses() {
        assert_eq!(serde_json::to_value(FileStatus::Paused).unwrap(), "paused");
        assert_eq!(
            serde_json::to_value(FileStatus::Cancelled).unwrap(),
            "cancelled"
        );
    }
}
