//! Health Check and System Monitoring Commands
//!
//! Thin command controllers for health checks and system statistics following DDD pattern.
//! Provides endpoints for monitoring application health, system resources, version info,
//! and model status with rate limiting to prevent resource exhaustion.
//!
//! # Commands (3 total)
//!
//! - `health_check` - Comprehensive system health check (database, models, services)
//! - `get_system_stats` - System statistics (documents, chunks, memory usage)
//! - `get_version` - Application version string
//!
//! # Gateway Pattern
//!
//! All commands are routed through the Gateway pattern which provides async dispatch.
//! The `*_impl` functions are the async implementations called by the gateway.
//!
//! ## Security Features
//!
//! - **Rate Limiting (CWE-770)**: Health checks limited to prevent DoS
//! - **Audit Logging (CWE-778)**: All health checks and stats queries logged

use crate::interfaces::di::Container;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub database: bool,
    pub embedding_model: bool,
    pub llm: bool,
    pub timestamp: String,
}

/// Returns application version string from Cargo.toml
///
/// This is a synchronous command returning a compile-time constant.
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// =============================================================================
// GATEWAY IMPL FUNCTIONS - Async implementations for gateway dispatch
// =============================================================================

/// Health check implementation for gateway pattern
///
/// This async function is called directly by the gateway.
pub async fn health_check_impl(container: &Container) -> std::result::Result<String, String> {
    let use_case = container.health_check_use_case();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .health_check
        .check_rate_limit("global")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let response = use_case
        .execute()
        .await
        .map_err(|e| format!("Health check failed: {}", e))?;

    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::HealthCheck,
        "system_health",
        "status" => response.status.as_str()
    )
    .await
    .ok();

    serde_json::to_string(&response).map_err(|e| format!("Serialization error: {}", e))
}

/// System stats implementation for gateway pattern
///
/// This async function is called directly by the gateway.
pub async fn get_system_stats_impl(container: &Container) -> std::result::Result<String, String> {
    let use_case = container.get_system_stats_use_case();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .health_check
        .check_rate_limit("global")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let stats = use_case
        .execute()
        .await
        .map_err(|e| format!("Stats collection failed: {}", e))?;

    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::HealthCheck,
        "system_stats",
        "doc_count" => stats.total_documents.to_string(),
        "chunk_count" => stats.total_chunks.to_string()
    )
    .await
    .ok();

    serde_json::to_string(&stats).map_err(|e| format!("Serialization error: {}", e))
}
