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
//! - **Audit Logging**: All update checks logged for security compliance
//!
//! # Known Limitations
//!
//! **⚠️ Version Info Unthrottled**: Update checks are rate limited, but
//! `get_version_info` is currently not throttled.
//!
//! # Architecture
//!
//! Commands delegate to update use cases which:
//! - Query GitHub API for latest release
//! - Compare versions (semantic versioning)
//! - Provide update URLs and release notes

use crate::features::updates::dto::{UpdateInfoDto, VersionInfoDto};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use tauri::State;

/// Checks for available application updates
///
/// Queries GitHub releases API to determine if a newer version is available. Returns
/// update information including version, download URL, and release notes. Audit logged
/// for security compliance.
///
/// # Arguments
///
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(UpdateInfoDto)` - Update information (available flag, version, URL, notes)
/// * `Err(AppError)` - Update check failed (network error, API error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface UpdateInfo {
///   available: boolean;       // Update available flag
///   version: string;          // Latest version (e.g., "1.2.0")
///   currentVersion: string;   // Current version (e.g., "1.1.0")
///   downloadUrl: string;      // Download URL for new version
///   releaseNotes: string;     // Markdown release notes
///   publishedAt: string;      // ISO 8601 timestamp
/// }
///
/// // Check for updates
/// const updateInfo = await invoke<UpdateInfo>('check_for_updates');
///
/// if (updateInfo.available) {
///   console.log(`Update available: ${updateInfo.version}`);
///   showUpdateNotification({
///     title: `Version ${updateInfo.version} available`,
///     message: 'Click to download',
///     url: updateInfo.downloadUrl
///   });
/// } else {
///   console.log('Application is up to date');
/// }
///
/// // Periodic update check
/// setInterval(async () => {
///   const updateInfo = await invoke<UpdateInfo>('check_for_updates');
///   if (updateInfo.available) {
///     showUpdateBanner(updateInfo);
///   }
/// }, 24 * 60 * 60 * 1000); // Daily
///
/// // Display release notes
/// if (updateInfo.available) {
///   displayReleaseNotes(updateInfo.releaseNotes);
/// }
/// ```
///
/// # Update Detection Logic
///
/// 1. Query GitHub API for latest release
/// 2. Parse semantic version (major.minor.patch)
/// 3. Compare with current version (from Cargo.toml)
/// 4. Return `available: true` if newer version exists
///
/// # Security
///
/// **Audit Logging (CWE-778)**: All update checks logged with availability status
///
/// **⚠️ Rate Limiting Disabled**: Currently commented out (TODO)
/// - **Risk**: No protection against update check spam
/// - **Mitigation**: Frontend should implement client-side rate limiting
///
/// # Use Cases
///
/// - **Manual Check**: User clicks "Check for Updates" button
/// - **Background Check**: Periodic automatic update detection
/// - **Startup Check**: Check for updates on application launch
/// - **Settings Display**: Show current/latest version in settings
///
/// # Performance
///
/// - **Check Time**: ~100-500ms (depends on GitHub API response)
/// - **Network**: Requires internet connectivity
/// - **Caching**: GitHub API responses cached by GitHub CDN
/// - **Async**: Non-blocking operation
///
/// # Architecture
///
/// Thin controller delegating to `CheckForUpdatesUseCase` (DDD pattern)
pub async fn check_for_updates(container: State<'_, Container>) -> Result<UpdateInfoDto> {
    // 1. Rate limiting (CWE-770 mitigation)
    container
        .security_context()
        .rate_limiters()
        .health_check
        .check_rate_limit("updates")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // 2. Get use case from container
    let use_case = container.check_for_updates_use_case();

    // 3. Execute use case
    let update_info = use_case.execute().await?;

    // 4. Audit logging (CWE-778 compliance)
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::UpdateChecked,
        "updates",
        "available" => update_info.available.to_string()
    )
    .await
    .ok();

    Ok(update_info)
}

/// Returns detailed current version information
///
/// Retrieves version metadata including semantic version, build date (if available),
/// and commit hash (if available). Used for displaying version details in UI and
/// troubleshooting.
///
/// # Arguments
///
/// * `container` - Service container with use cases
///
/// # Returns
///
/// * `Ok(VersionInfoDto)` - Detailed version information
/// * `Err(AppError)` - Version retrieval failed (rare)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface VersionInfo {
///   version: string;          // Semantic version (e.g., "1.1.0")
///   buildDate: string | null; // ISO 8601 build timestamp (optional)
///   commitHash: string | null; // Git commit hash (short, optional)
/// }
///
/// // Get version info
/// const versionInfo = await invoke<VersionInfo>('get_version_info');
///
/// // Display in About dialog
/// document.getElementById('version').textContent = versionInfo.version;
/// document.getElementById('build-date').textContent = versionInfo.buildDate;
/// document.getElementById('commit').textContent = versionInfo.commitHash;
///
/// // Copy to clipboard for bug reports
/// const copyVersionInfo = async () => {
///   const info = await invoke<VersionInfo>('get_version_info');
///   const text = `
///     Version: ${info.version}
///     Build Date: ${info.buildDate}
///     Commit: ${info.commitHash}
///   `;
///   navigator.clipboard.writeText(text);
/// };
///
/// // Display in settings
/// const displayVersionInfo = async () => {
///   const info = await invoke<VersionInfo>('get_version_info');
///   return `v${info.version} (${info.commitHash})`;
/// };
/// ```
///
/// # Version Information
///
/// - **version**: Semantic version string (major.minor.patch)
/// - **buildDate**: Optional build date string (currently not populated)
/// - **commitHash**: Optional commit hash string (currently not populated)
///
/// # Security
///
/// **No Rate Limiting**: Version info queries are intentionally unthrottled
/// - **Risk**: Minimal (read-only, constant-time access)
///
/// # Use Cases
///
/// - **About Dialog**: Display version in "About" window
/// - **Bug Reports**: Include version info in bug report templates
/// - **Troubleshooting**: Verify installed version during support
/// - **Version Badge**: Show version in UI footer or title bar
///
/// # Performance
///
/// - **Access Time**: ~1-5ms (reads from in-memory data)
/// - **No I/O**: No network or file access
/// - **Async**: Non-blocking operation
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
    container
        .security_context()
        .rate_limiters()
        .health_check
        .check_rate_limit("updates")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;
    let use_case = container.check_for_updates_use_case();
    let update_info = use_case.execute().await?;

    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::UpdateChecked,
        "updates",
        "available" => update_info.available.to_string()
    )
    .await
    .ok();

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
