use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfoDto {
    pub path: String,
    pub name: String,
    pub created_at: String,
    pub version: String,
    pub file_count: usize,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateBackupResultDto {
    pub backup_path: String,
    pub size: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupResultDto {
    pub success: bool,
    pub restored_count: usize,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListBackupsResultDto {
    pub backups: Vec<BackupInfoDto>,
}

// ---------------------------------------------------------------------------
// Encrypted off-device archive
//
// Design: docs/design/2026-09-16-encrypted-backup-archive.md. `outcome` and
// `availability` are plain strings carrying the documented literals rather
// than Rust enums: specta renders a `String` identically on both sides and
// the frontend compares them as literals anyway.
// ---------------------------------------------------------------------------

/// One completed archive run.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveRunDto {
    pub path: String,
    pub created_at: String,
    pub size: u64,
    pub duration_ms: u64,
}

/// The last archive failure, so the UI can explain why backups stopped.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveErrorDto {
    pub at: String,
    pub message: String,
}

/// An archive sitting in the destination folder.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveFileDto {
    pub path: String,
    pub name: String,
    pub created_at: String,
    pub size: u64,
    /// `"local"` | `"placeholder"` | `"unknown"`.
    pub availability: String,
}

/// Everything the off-device backup panel renders from.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStatusDto {
    /// A key envelope exists — setup finished at least once.
    pub configured: bool,
    pub destination: Option<String>,
    /// `"iCloud Drive"`, `"Dropbox"`, … when the destination is recognisably
    /// inside a synced folder.
    pub destination_provider: Option<String>,
    /// The configured destination no longer exists (unplugged disk, renamed
    /// folder).
    pub destination_missing: bool,
    pub keep_count: u32,
    pub has_passphrase: bool,
    pub recovery_confirmed: bool,
    pub last_success: Option<ArchiveRunDto>,
    pub last_error: Option<ArchiveErrorDto>,
    /// Set when the *live* database sits in a synced folder, which corrupts
    /// SQLite. The UI shows a warning banner.
    pub data_dir_cloud_provider: Option<String>,
    /// Newest first.
    pub archives: Vec<ArchiveFileDto>,
}

/// The recovery code, shown once, plus the word positions the user must
/// retype to prove they wrote it down.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSetupDto {
    /// 24 BIP-39 English words.
    pub recovery_words: Vec<String>,
    /// Three zero-based indices into `recovery_words`.
    pub confirm_indices: Vec<u32>,
}

/// One "word 7 was ___" answer.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WordConfirmationDto {
    pub index: u32,
    pub word: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmArchiveSetupRequestDto {
    pub confirmations: Vec<WordConfirmationDto>,
    /// Optional second unlock path. At least 8 characters when present.
    #[serde(default)]
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetArchiveKeepCountRequestDto {
    pub keep_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetArchivePassphraseRequestDto {
    /// `None` removes the passphrase slot; the recovery code always remains.
    #[serde(default)]
    pub passphrase: Option<String>,
}

/// The source file is picked in Rust, so the renderer only ever supplies the
/// secret — never a path.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreArchiveRequestDto {
    #[serde(default)]
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreArchiveResultDto {
    /// `"restored"` | `"cancelled"` | `"needs_secret"` | `"not_hydrated"`.
    pub outcome: String,
    pub message: Option<String>,
    pub restart_required: bool,
    pub reembed_required: bool,
    /// Set when an existing non-empty vault forced the restore beside it.
    pub vault_restored_to: Option<String>,
    pub files_restored: u64,
}

/// The documented `outcome` literals.
pub mod archive_outcome {
    pub const RESTORED: &str = "restored";
    pub const CANCELLED: &str = "cancelled";
    pub const NEEDS_SECRET: &str = "needs_secret";
    pub const NOT_HYDRATED: &str = "not_hydrated";
}

/// The documented `availability` literals.
pub mod archive_availability {
    pub const LOCAL: &str = "local";
    pub const PLACEHOLDER: &str = "placeholder";
    pub const UNKNOWN: &str = "unknown";
}
