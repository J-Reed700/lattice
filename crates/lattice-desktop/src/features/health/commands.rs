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

use crate::features::health::dto::HealthCheckResponseDto;
pub use crate::features::health::plugin::types::HealthStatus;
use crate::interfaces::di::Container;
use lattice_core::application::contracts::system_stats::SystemStatsDto;

/// Returns application version string from Cargo.toml
///
/// This is a synchronous command returning a compile-time constant.
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Health check implementation for gateway pattern
///
/// This async function is called directly by the gateway.
pub async fn health_check_impl(
    container: &Container,
) -> std::result::Result<HealthCheckResponseDto, String> {
    let use_case = container.health_check_use_case();
    let llm_ready = container
        .loaded_models()
        .iter()
        .any(|model| model.degraded.is_none());

    use_case
        .execute(llm_ready)
        .await
        .map_err(|e| format!("Health check failed: {}", e))
}

/// System stats implementation for gateway pattern
///
/// This async function is called directly by the gateway.
pub async fn get_system_stats_impl(
    container: &Container,
) -> std::result::Result<SystemStatsDto, String> {
    let use_case = container.get_system_stats_use_case();

    use_case
        .execute()
        .await
        .map_err(|e| format!("Stats collection failed: {}", e))
}
