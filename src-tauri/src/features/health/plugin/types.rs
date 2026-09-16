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
