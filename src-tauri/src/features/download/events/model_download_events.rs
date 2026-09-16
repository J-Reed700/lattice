use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSpec {
    pub file_id: String,
    pub file_name: String,
    pub file_size: u64,
}

/// Model download requested event
///
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadRequestedEvent {
    pub model_id: String,
    pub model_name: String,
    pub files: Vec<FileSpec>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadStartedEvent {
    pub model_id: String,
    pub model_name: String,
    pub total_files: usize,
    pub total_size: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadQueuedEvent {
    pub model_id: String,
    pub file_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadStartedEvent {
    pub model_id: String,
    pub file_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadProgressEvent {
    pub model_id: String,
    pub file_id: String,
    pub file_name: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadCompletedEvent {
    pub model_id: String,
    pub file_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadFailedEvent {
    pub model_id: String,
    pub file_id: String,
    pub file_name: String,
    pub error: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadCompletedEvent {
    pub model_id: String,
    pub model_name: String,
    pub total_size: u64,
    pub file_count: usize,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadFailedEvent {
    pub model_id: String,
    pub model_name: String,
    pub error: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelDownloadEvent {
    ModelDownloadRequested(ModelDownloadRequestedEvent),
    ModelDownloadStarted(ModelDownloadStartedEvent),
    FileDownloadQueued(FileDownloadQueuedEvent),
    FileDownloadStarted(FileDownloadStartedEvent),
    FileDownloadProgress(FileDownloadProgressEvent),
    FileDownloadCompleted(FileDownloadCompletedEvent),
    FileDownloadFailed(FileDownloadFailedEvent),
    ModelDownloadCompleted(ModelDownloadCompletedEvent),
    ModelDownloadFailed(ModelDownloadFailedEvent),
}

impl ModelDownloadEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::ModelDownloadRequested(_) => "model_download_requested",
            Self::ModelDownloadStarted(_) => "model_download_started",
            Self::FileDownloadQueued(_) => "file_download_queued",
            Self::FileDownloadStarted(_) => "file_download_started",
            Self::FileDownloadProgress(_) => "file_download_progress",
            Self::FileDownloadCompleted(_) => "file_download_completed",
            Self::FileDownloadFailed(_) => "file_download_failed",
            Self::ModelDownloadCompleted(_) => "model_download_completed",
            Self::ModelDownloadFailed(_) => "model_download_failed",
        }
    }

    pub fn model_id(&self) -> &str {
        match self {
            Self::ModelDownloadRequested(e) => &e.model_id,
            Self::ModelDownloadStarted(e) => &e.model_id,
            Self::FileDownloadQueued(e) => &e.model_id,
            Self::FileDownloadStarted(e) => &e.model_id,
            Self::FileDownloadProgress(e) => &e.model_id,
            Self::FileDownloadCompleted(e) => &e.model_id,
            Self::FileDownloadFailed(e) => &e.model_id,
            Self::ModelDownloadCompleted(e) => &e.model_id,
            Self::ModelDownloadFailed(e) => &e.model_id,
        }
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::ModelDownloadRequested(e) => e.timestamp,
            Self::ModelDownloadStarted(e) => e.timestamp,
            Self::FileDownloadQueued(e) => e.timestamp,
            Self::FileDownloadStarted(e) => e.timestamp,
            Self::FileDownloadProgress(e) => e.timestamp,
            Self::FileDownloadCompleted(e) => e.timestamp,
            Self::FileDownloadFailed(e) => e.timestamp,
            Self::ModelDownloadCompleted(e) => e.timestamp,
            Self::ModelDownloadFailed(e) => e.timestamp,
        }
    }
}
