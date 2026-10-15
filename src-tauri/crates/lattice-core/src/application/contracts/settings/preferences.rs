//! Interface, sync, backup, privacy, vault and onboarding settings and their defaults.

use serde::{Deserialize, Serialize};

/// UI settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UISettingsDto {
    /// Theme (light/dark/system)
    pub theme: String,

    /// Font size in pixels
    pub font_size: u32,

    /// Show preview pane
    pub show_preview: bool,

    /// Results per page
    pub results_per_page: u32,

    /// Enable animations
    pub enable_animations: bool,
}

/// Sync settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettingsDto {
    /// Sync feature enabled
    pub sync_enabled: bool,

    /// Sync server URL
    pub sync_url: String,

    /// Sync interval in minutes
    pub sync_interval_minutes: u32,

    /// Automatically sync on changes
    pub auto_sync: bool,

    /// Sync on application startup
    pub sync_on_startup: bool,
}

/// Backup settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettingsDto {
    /// Auto-backup enabled
    pub auto_backup_enabled: bool,

    /// Backup frequency (daily/weekly/monthly)
    pub backup_frequency: String,

    /// Number of days to retain backups
    pub backup_retention_days: u32,

    /// Path to backup directory
    pub backup_path: String,

    /// Compress backups
    pub compress_backups: bool,
}

/// Privacy settings — defaults to opt-out for both flags.
///
/// These flags must be read by the backend before emitting telemetry or
/// crash reports. Source of truth lives here, not in the frontend
/// localStorage before moving to backend-owned settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct PrivacySettingsDto {
    /// Send anonymous usage statistics. Off by default.
    pub telemetry_enabled: bool,

    /// Send crash reports. Off by default.
    pub crash_reporting: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct VaultSettingsDto {
    /// Empty string = `~/Lattice`.
    #[serde(default)]
    pub vault_path: String,

    #[serde(default)]
    pub enabled: bool,

    /// Reverse-direction sync: import external `.md` edits back into SQL.
    #[serde(default)]
    pub watch_external_changes: bool,
}

/// Onboarding state — what the user has already been through.
///
/// Persisted first-run state, and the single source of truth for the
/// first-run gate.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct OnboardingSettingsDto {
    /// True once the user has installed the recommended models, chosen their
    /// own, or said "not now". The first-run modal never re-opens while set.
    #[serde(default)]
    pub first_run_dismissed: bool,
}

impl Default for UISettingsDto {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            font_size: 14,
            show_preview: true,
            results_per_page: 10,
            enable_animations: true,
        }
    }
}

impl Default for SyncSettingsDto {
    fn default() -> Self {
        Self {
            sync_enabled: false,
            sync_url: String::new(),
            sync_interval_minutes: 30,
            auto_sync: false,
            sync_on_startup: false,
        }
    }
}

impl Default for BackupSettingsDto {
    fn default() -> Self {
        Self {
            auto_backup_enabled: false,
            backup_frequency: "daily".to_string(),
            backup_retention_days: 30,
            backup_path: String::new(),
            compress_backups: true,
        }
    }
}
