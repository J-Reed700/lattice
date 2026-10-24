//! Health Check and System Monitoring Commands
//!
//! Command handlers for health checks and system statistics.
//! Provides endpoints for monitoring application health, system resources, version info,
//! and model status.

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

/// Health check implementation
pub async fn health_check_impl(container: &Container) -> std::result::Result<String, String> {
    let use_case = container.health_check_use_case();
    let llm_ready = container
        .loaded_models()
        .iter()
        .any(|model| model.degraded.is_none());

    let response = use_case
        .execute(llm_ready)
        .await
        .map_err(|e| format!("Health check failed: {}", e))?;

    serde_json::to_string(&response).map_err(|e| format!("Serialization error: {}", e))
}

/// System stats implementation
pub async fn get_system_stats_impl(container: &Container) -> std::result::Result<String, String> {
    let use_case = container.get_system_stats_use_case();

    let stats = use_case
        .execute()
        .await
        .map_err(|e| format!("Stats collection failed: {}", e))?;

    serde_json::to_string(&stats).map_err(|e| format!("Serialization error: {}", e))
}
