//! Health Check and System Monitoring Commands
//!
//! Thin command controllers for health checks and system statistics following DDD pattern.
//! Provides endpoints for monitoring application health, system resources, version info,
//! and model status.
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

use crate::interfaces::di::Container;
use serde::{Deserialize, Serialize};

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

/// Health check implementation for gateway pattern
///
/// This async function is called directly by the gateway.
pub async fn health_check_impl(container: &Container) -> std::result::Result<String, String> {
    let use_case = container.health_check_use_case();

    let response = use_case
        .execute()
        .await
        .map_err(|e| format!("Health check failed: {}", e))?;

    serde_json::to_string(&response).map_err(|e| format!("Serialization error: {}", e))
}

/// System stats implementation for gateway pattern
///
/// This async function is called directly by the gateway.
pub async fn get_system_stats_impl(container: &Container) -> std::result::Result<String, String> {
    let use_case = container.get_system_stats_use_case();

    let stats = use_case
        .execute()
        .await
        .map_err(|e| format!("Stats collection failed: {}", e))?;

    serde_json::to_string(&stats).map_err(|e| format!("Serialization error: {}", e))
}
