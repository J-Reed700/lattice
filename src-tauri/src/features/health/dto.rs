//! Health check DTOs.
//!
//! Data transfer objects for system health and status monitoring.

use serde::{Deserialize, Serialize};

/// Response for health check operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponseDto {
    /// Overall system status: "healthy", "degraded", or "unhealthy"
    pub status: String,
    /// Database connection health
    pub database: bool,
    /// Embedding model availability
    pub embedding_model: bool,
    /// LLM service availability
    pub llm: bool,
    /// ISO 8601 timestamp of the health check
    pub timestamp: String,
}

/// System statistics response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatsDto {
    /// Total number of indexed documents
    pub total_documents: i64,
    /// Total number of chunks across all documents
    pub total_chunks: i64,
    /// Total number of tags in the system
    pub total_tags: i64,
    /// Total storage size in bytes
    pub storage_size_bytes: i64,
}
