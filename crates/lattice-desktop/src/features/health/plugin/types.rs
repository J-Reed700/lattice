//! Health plugin DTOs with TypeScript generation

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct HealthStatus {
    pub status: String,
    pub database: bool,
    pub embedding_model: bool,
    pub llm: bool,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SystemStats {
    pub total_documents: i64,
    pub total_chunks: i64,
    pub total_tags: i64,
    pub storage_size_bytes: i64,
}

/// One line of the backend's file log. Lines that do not match the log format
/// carry only `message` (the raw line) and an `Unknown` level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct BackendLogEntry {
    pub timestamp: Option<String>,
    pub level: BackendLogLevel,
    pub target: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum BackendLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Unknown,
}
