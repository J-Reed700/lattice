//! Application Update Management Commands
//!
//! Thin command controllers for checking application updates and retrieving version
//! information following DDD pattern. Enables automatic update detection and user
//! notification of available updates.
//!
//! # Commands (2 total)
//!
//! - `check_for_updates` - Check for newer application versions
//! - `get_version_info` - Get detailed current version information
//!
//! # Update Strategy
//!
//! - **Update Source**: GitHub releases (via API)
//! - **Check Frequency**: User-initiated or periodic background checks
//! - **Update Mechanism**: User downloads and installs manually
//!
//! # Architecture
//!
//! Commands delegate to update use cases which:
//! - Query GitHub API for latest release
//! - Compare versions (semantic versioning)
//! - Provide update URLs and release notes

use crate::features::updates::dto::{UpdateInfoDto, VersionInfoDto};
use crate::interfaces::di::Container;
use lattice_core::shared::error::Result;
use tauri::State;

/// Checks for available application updates
///
/// Queries GitHub releases API to determine if a newer version is available. Returns
/// update information including version, download URL, and release notes.
///
/// # Update Detection Logic
///
/// 1. Query GitHub API for latest release
/// 2. Parse semantic version (major.minor.patch)
/// 3. Compare with current version (from Cargo.toml)
/// 4. Return `available: true` if newer version exists
///
/// # Architecture
///
/// Thin controller delegating to `CheckForUpdatesUseCase` (DDD pattern)
pub async fn check_for_updates(container: State<'_, Container>) -> Result<UpdateInfoDto> {
    let use_case = container.check_for_updates_use_case();
    let update_info = use_case.execute().await?;

    Ok(update_info)
}

/// Returns detailed current version information
///
/// Retrieves version metadata including semantic version, build date (if available),
/// and commit hash (if available). Used for displaying version details in UI and
/// troubleshooting.
///
/// # Version Information
///
/// - **version**: Semantic version string (major.minor.patch)
/// - **buildDate**: Optional build date string (currently not populated)
/// - **commitHash**: Optional commit hash string (currently not populated)
///
/// # Architecture
///
/// Thin controller delegating to `GetCurrentVersionUseCase` (DDD pattern)
pub async fn get_version_info(container: State<'_, Container>) -> Result<VersionInfoDto> {
    // 1. Get use case from container
    let use_case = container.get_version_info_use_case();

    // 2. Execute use case
    let version_info = use_case.execute().await?;

    Ok(version_info)
}

/// Check for updates implementation for gateway pattern
///
/// This async function is called directly by the gateway adapter.
/// Takes &Container instead of State<'_, Container>.
pub async fn check_for_updates_impl(container: &Container) -> Result<UpdateInfoDto> {
    let use_case = container.check_for_updates_use_case();
    let update_info = use_case.execute().await?;

    Ok(update_info)
}

/// Get version info implementation for gateway pattern
///
/// This async function is called directly by the gateway adapter.
/// Takes &Container instead of State<'_, Container>.
pub async fn get_version_info_impl(container: &Container) -> Result<VersionInfoDto> {
    let use_case = container.get_version_info_use_case();
    let version_info = use_case.execute().await?;

    Ok(version_info)
}
