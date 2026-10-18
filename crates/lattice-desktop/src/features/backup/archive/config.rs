//! Local state for the encrypted archive feature.
//!
//! `<app_data_dir>/backup/archive-config.json` holds everything the app needs
//! to keep writing archives: the key envelope (so a restore is possible from
//! any archive even if this file is lost — the envelope is copied into every
//! header), the chosen destination, retention, and the last run's outcome.
//!
//! It deliberately lives outside `settings.json`: it is backup-slice state,
//! not user preferences, and it must survive a settings reset.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::format::{ArchiveError, KeyEnvelope};

/// Directory under `app_data_dir` that holds all archive-local state.
pub const BACKUP_STATE_DIR: &str = "backup";
/// File name of the archive configuration.
pub const CONFIG_FILE_NAME: &str = "archive-config.json";
/// Current `version` value written into the file.
pub const CONFIG_VERSION: u8 = 1;
/// How many archives to keep in the destination folder by default.
pub const DEFAULT_KEEP_COUNT: u32 = 5;
/// Inclusive bounds accepted by `plugin_set_archive_keep_count`.
pub const MIN_KEEP_COUNT: u32 = 1;
pub const MAX_KEEP_COUNT: u32 = 50;

/// A successful archive run, as recorded in the config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveRunRecord {
    pub path: String,
    pub created_at: String,
    pub size: u64,
    pub duration_ms: u64,
}

/// The last failure, kept so the UI can show why backups stopped working.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveErrorRecord {
    pub at: String,
    pub message: String,
}

/// Contents of `archive-config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveConfig {
    #[serde(default = "default_version")]
    pub version: u8,
    /// The folder the user picked. Archives land in
    /// `<destination>/Lattice Backups/`. `None` means "set up but turned off".
    #[serde(default)]
    pub destination: Option<PathBuf>,
    #[serde(default = "default_keep_count")]
    pub keep_count: u32,
    /// Copied verbatim into every archive header.
    #[serde(default)]
    pub envelope: KeyEnvelope,
    /// RFC 3339 UTC, set when the user retyped the requested recovery words.
    #[serde(default)]
    pub recovery_confirmed_at: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_success: Option<ArchiveRunRecord>,
    #[serde(default)]
    pub last_error: Option<ArchiveErrorRecord>,
}

fn default_version() -> u8 {
    CONFIG_VERSION
}

fn default_keep_count() -> u32 {
    DEFAULT_KEEP_COUNT
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            destination: None,
            keep_count: DEFAULT_KEEP_COUNT,
            envelope: KeyEnvelope::default(),
            recovery_confirmed_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_success: None,
            last_error: None,
        }
    }
}

impl ArchiveConfig {
    /// `<app_data_dir>/backup/archive-config.json`.
    pub fn path(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join(BACKUP_STATE_DIR).join(CONFIG_FILE_NAME)
    }

    /// The directory holding archive-local state (config, key fallback,
    /// scratch space).
    pub fn state_dir(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join(BACKUP_STATE_DIR)
    }

    /// Read the config. `Ok(None)` when the feature has never been set up.
    ///
    /// A file that exists but cannot be parsed is an error rather than a
    /// silent `None`: the envelope inside it is the only local copy of the
    /// wrapped master key, and quietly ignoring it would look like "not
    /// configured" and invite the user to generate a *second* key.
    pub async fn load(app_data_dir: &Path) -> Result<Option<Self>, ArchiveError> {
        let path = Self::path(app_data_dir);
        let raw = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ArchiveError::Io(e)),
        };
        let config: Self = serde_json::from_slice(&raw).map_err(|e| {
            ArchiveError::Other(format!(
                "{} is not readable ({e}); it was not modified",
                path.display()
            ))
        })?;
        Ok(Some(config))
    }

    /// Write the config atomically: `.tmp` alongside, then rename. A crash
    /// mid-write leaves the previous config intact rather than a truncated
    /// file with half an envelope in it.
    pub async fn save(&self, app_data_dir: &Path) -> Result<(), ArchiveError> {
        let path = Self::path(app_data_dir);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| ArchiveError::Other(format!("could not serialize archive config: {e}")))?;
        let tmp = path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &json).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(())
    }

    /// A private, empty directory for one archive run's working files.
    ///
    /// Always under the app data directory, never the system temp dir: the
    /// scratch copy of the database is a full plaintext copy of the user's
    /// corpus and belongs on the same volume, under the same permissions, as
    /// the live database.
    pub fn scratch_dir(app_data_dir: &Path) -> PathBuf {
        Self::state_dir(app_data_dir)
            .join("scratch")
            .join(uuid::Uuid::new_v4().to_string())
    }

    /// True once a key envelope exists — the definition of "configured".
    pub fn is_configured(&self) -> bool {
        !self.envelope.slots.is_empty()
    }

    /// True when archives can actually be written right now.
    pub fn is_active(&self) -> bool {
        self.is_configured() && self.destination.is_some()
    }
}

/// Clamp-free validation of a requested retention count.
pub fn validate_keep_count(keep_count: u32) -> Result<u32, ArchiveError> {
    if !(MIN_KEEP_COUNT..=MAX_KEEP_COUNT).contains(&keep_count) {
        return Err(ArchiveError::Other(format!(
            "keep count must be between {MIN_KEEP_COUNT} and {MAX_KEEP_COUNT}"
        )));
    }
    Ok(keep_count)
}

/// Reject a destination that is the app data directory or inside it.
///
/// Writing archives into `app_data_dir` would put the backup on the same
/// disk, in the same directory tree, as the thing it is backing up — and
/// worse, the next archive would try to pack the previous one.
pub fn validate_destination(destination: &Path, app_data_dir: &Path) -> Result<(), ArchiveError> {
    if destination.starts_with(app_data_dir) {
        return Err(ArchiveError::Other(
            "choose a folder outside Lattice's own data directory — ideally one your cloud drive or an external disk syncs".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::backup::archive::format::KeySlot;

    fn envelope() -> KeyEnvelope {
        KeyEnvelope {
            slots: vec![KeySlot::Recovery {
                salt: "c2FsdA==".into(),
                nonce: "bm9uY2U=".into(),
                wrapped_key: "d3JhcA==".into(),
            }],
            kcv: "a2N2".into(),
        }
    }

    #[tokio::test]
    async fn load_returns_none_before_setup() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(ArchiveConfig::load(dir.path()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let config = ArchiveConfig {
            version: CONFIG_VERSION,
            destination: Some(PathBuf::from("/Users/x/CloudDocs")),
            keep_count: 7,
            envelope: envelope(),
            recovery_confirmed_at: Some("2026-09-16T12:00:00Z".to_string()),
            created_at: "2026-09-16T11:00:00Z".to_string(),
            last_success: Some(ArchiveRunRecord {
                path: "/Users/x/CloudDocs/Lattice Backups/a.lattice-backup".to_string(),
                created_at: "2026-09-16T12:00:00Z".to_string(),
                size: 123,
                duration_ms: 4500,
            }),
            last_error: Some(ArchiveErrorRecord {
                at: "2026-09-16T13:00:00Z".to_string(),
                message: "disk full".to_string(),
            }),
        };

        config.save(dir.path()).await.unwrap();
        let loaded = ArchiveConfig::load(dir.path()).await.unwrap();

        assert_eq!(loaded, Some(config));
        // The atomic write leaves nothing behind.
        assert!(!ArchiveConfig::path(dir.path())
            .with_extension("json.tmp")
            .exists());
    }

    #[tokio::test]
    async fn save_overwrites_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = ArchiveConfig {
            envelope: envelope(),
            ..Default::default()
        };
        config.save(dir.path()).await.unwrap();
        config.keep_count = 42;
        config.save(dir.path()).await.unwrap();

        let loaded = ArchiveConfig::load(dir.path()).await.unwrap().unwrap();
        assert_eq!(loaded.keep_count, 42);
    }

    #[tokio::test]
    async fn unparseable_config_is_an_error_not_a_fresh_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = ArchiveConfig::path(dir.path());
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, b"{ not json").await.unwrap();

        assert!(ArchiveConfig::load(dir.path()).await.is_err());
    }

    #[test]
    fn configured_needs_an_envelope_active_needs_a_destination() {
        let mut config = ArchiveConfig::default();
        assert!(!config.is_configured());
        assert!(!config.is_active());

        config.envelope = envelope();
        assert!(config.is_configured());
        assert!(!config.is_active());

        config.destination = Some(PathBuf::from("/tmp/dest"));
        assert!(config.is_active());
    }

    #[test]
    fn keep_count_bounds() {
        assert!(validate_keep_count(0).is_err());
        assert_eq!(validate_keep_count(1).unwrap(), 1);
        assert_eq!(validate_keep_count(50).unwrap(), 50);
        assert!(validate_keep_count(51).is_err());
    }

    #[test]
    fn destination_inside_app_data_dir_is_rejected() {
        let app = Path::new("/Users/x/Library/Application Support/Lattice");
        assert!(validate_destination(app, app).is_err());
        assert!(validate_destination(&app.join("backups"), app).is_err());
        assert!(validate_destination(Path::new("/Users/x/Dropbox"), app).is_ok());
    }
}
