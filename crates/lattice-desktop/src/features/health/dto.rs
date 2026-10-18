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
