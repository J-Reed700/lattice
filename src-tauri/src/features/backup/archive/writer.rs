//! Producing one `.lattice-backup` file.
//!
//! The sequence is deliberately boring: snapshot the database into a scratch
//! directory, stream `tar(zstd(...))` through the AEAD encryptor into a
//! `.part` file next to its final name, `fsync`, rename. A crash at any point
//! leaves either the previous archive or a `.part` file that retention
//! ignores — never a half-written archive under a name that looks complete.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::application::ports::SettingsRepositoryPort;

use super::config::ArchiveConfig;
use super::crypto::{self, MasterKey};
use super::format::{
    self, ArchiveError, ArchiveHeader, CHUNK_SIZE, CIPHER_ID, COMPRESSION_ID, FORMAT_VERSION,
    PART_SUFFIX, STREAM_NONCE_LEN,
};
use super::key_store::MasterKeyStore;
use super::placeholder::{self, FileAvailability};
use super::snapshot::{self, ArchiveInputs, ProgressEvent};

use base64::prelude::{Engine as _, BASE64_STANDARD};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const FILE_SALT_LEN: usize = 16;

/// The message shown when this device has an envelope but no master key —
/// the state you land in after copying `archive-config.json` to a new machine
/// without restoring. It has to tell the user the way out.
pub const MISSING_KEY_MESSAGE: &str =
    "backup key missing on this device: restore once with your passphrase or recovery code";

/// One completed archive.
#[derive(Debug, Clone)]
pub struct ArchiveRun {
    pub path: PathBuf,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub size: u64,
    pub duration_ms: u64,
}

/// One archive file found in the destination folder.
#[derive(Debug, Clone)]
pub struct ArchiveFileInfo {
    pub path: PathBuf,
    pub name: String,
    /// RFC 3339 UTC, from the file's modification time.
    pub created_at: String,
    pub size: u64,
    pub availability: FileAvailability,
}

/// `<destination>/Lattice Backups`.
pub fn archive_dir(destination: &Path) -> PathBuf {
    destination.join(format::ARCHIVE_SUBDIR)
}

/// Does this path exist? Kept here so the orchestration layer never has to
/// ask the filesystem directly.
pub fn path_exists(path: &Path) -> bool {
    // repository-barrier-allow: the destination folder is a user-chosen
    // filesystem resource, not persisted application state.
    path.exists()
}

/// Every archive in `<destination>/Lattice Backups`, newest first.
///
/// Never touches file *contents*, so a folder full of evicted iCloud
/// placeholders lists instantly instead of triggering 5 GB of downloads.
pub fn list_archives(destination: &Path) -> Vec<ArchiveFileInfo> {
    let dir = archive_dir(destination);
    // repository-barrier-allow: enumerating the user's own backup folder.
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut archives: Vec<ArchiveFileInfo> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            if !format::is_archive_file_name(&name) {
                return None;
            }
            // repository-barrier-allow: size and mtime of the backup file itself.
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let created_at = meta
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                .unwrap_or_default();
            Some(ArchiveFileInfo {
                availability: placeholder::file_availability(&path),
                path,
                name,
                created_at,
                size: meta.len(),
            })
        })
        .collect();

    // Names are `lattice-backup-<YYYYMMDDTHHMMSSZ>`, so lexicographic
    // descending is chronological descending and does not depend on mtimes
    // that a sync client may have rewritten.
    archives.sort_by(|a, b| b.name.cmp(&a.name));
    archives
}

/// Writes archives. Holds no mutable state; every run re-reads the config so
/// a destination change between the scheduler tick and the run is honoured.
pub struct ArchiveWriter {
    pool: SqlitePool,
    app_data_dir: PathBuf,
    settings_repo: Arc<dyn SettingsRepositoryPort>,
    key_store: Arc<MasterKeyStore>,
    /// Content-addressed files library. Injected rather than resolved from
    /// `$HOME` inside the run, so a test can point it somewhere harmless
    /// instead of packing (and later restoring into) the developer's own
    /// library.
    files_root: Option<PathBuf>,
}

impl ArchiveWriter {
    pub fn new(
        pool: SqlitePool,
        app_data_dir: PathBuf,
        settings_repo: Arc<dyn SettingsRepositoryPort>,
        key_store: Arc<MasterKeyStore>,
        files_root: Option<PathBuf>,
    ) -> Self {
        Self {
            pool,
            app_data_dir,
            settings_repo,
            key_store,
            files_root,
        }
    }

    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    /// Write one archive, apply retention, and record the outcome in the
    /// config file so the UI can show it and the next run can explain itself.
    pub async fn create(&self) -> Result<ArchiveRun, ArchiveError> {
        let started = std::time::Instant::now();
        let mut config = ArchiveConfig::load(&self.app_data_dir)
            .await?
            .ok_or_else(|| {
                ArchiveError::Other("off-device backup has not been set up yet".to_string())
            })?;
        let destination = config.destination.clone().ok_or_else(|| {
            ArchiveError::Other("choose a backup folder before backing up".to_string())
        })?;
        if !config.is_configured() {
            return Err(ArchiveError::Other(
                "off-device backup has no key envelope; set it up again".to_string(),
            ));
        }

        let result = self.run(&config, &destination, started).await;

        match &result {
            Ok(run) => {
                config.last_success = Some(super::config::ArchiveRunRecord {
                    path: run.path.to_string_lossy().to_string(),
                    created_at: run.created_at.to_rfc3339(),
                    size: run.size,
                    duration_ms: run.duration_ms,
                });
                config.last_error = None;
            }
            Err(e) => {
                config.last_error = Some(super::config::ArchiveErrorRecord {
                    at: chrono::Utc::now().to_rfc3339(),
                    message: e.to_string(),
                });
            }
        }
        if let Err(e) = config.save(&self.app_data_dir).await {
            warn!(error = %e, "archive run finished but its outcome could not be recorded");
        }

        result
    }

    async fn run(
        &self,
        config: &ArchiveConfig,
        destination: &Path,
        started: std::time::Instant,
    ) -> Result<ArchiveRun, ArchiveError> {
        let master = self
            .key_store
            .load()?
            .filter(|key| crypto::kcv_matches(&config.envelope, key))
            .ok_or_else(|| ArchiveError::Other(MISSING_KEY_MESSAGE.to_string()))?;

        let scratch = ArchiveConfig::scratch_dir(&self.app_data_dir);
        tokio::fs::create_dir_all(&scratch).await?;

        let outcome = self
            .run_in_scratch(config, destination, &scratch, &master, started)
            .await;

        // The scratch copy of the database is a full plaintext copy of the
        // user's corpus. It does not get to outlive this call, success or not.
        if let Err(e) = tokio::fs::remove_dir_all(&scratch).await {
            warn!(error = %e, scratch = %scratch.display(), "could not remove archive scratch directory");
        }

        outcome
    }

    async fn run_in_scratch(
        &self,
        config: &ArchiveConfig,
        destination: &Path,
        scratch: &Path,
        master: &MasterKey,
        started: std::time::Instant,
    ) -> Result<ArchiveRun, ArchiveError> {
        let created_at = chrono::Utc::now();
        let snapshot_path = scratch.join("lattice.db");
        let report = snapshot::snapshot_database(&self.pool, &snapshot_path).await?;
        debug!(
            bytes = report.bytes,
            migration_version = ?report.migration_version,
            "database snapshot written"
        );

        let out_dir = archive_dir(destination);
        tokio::fs::create_dir_all(&out_dir).await?;

        let inputs = self
            .collect_inputs(&report, &out_dir, &created_at)
            .await
            .map_err(|e| ArchiveError::Other(format!("could not resolve backup inputs: {e}")))?;

        let final_path = out_dir.join(format::archive_file_name(created_at));
        let part_path = with_part_suffix(&final_path);

        let header = build_header(&created_at, config)?;
        let file_salt = BASE64_STANDARD
            .decode(&header.file_salt)
            .map_err(|e| ArchiveError::Other(format!("bad file salt: {e}")))?;
        let stream_nonce: [u8; STREAM_NONCE_LEN] = BASE64_STANDARD
            .decode(&header.stream_nonce)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| ArchiveError::Other("bad stream nonce".to_string()))?;
        let file_key = crypto::derive_file_key(master, &file_salt);

        let write_target = part_path.clone();
        let written = tokio::task::spawn_blocking(move || -> Result<u64, ArchiveError> {
            let mut file = std::fs::File::create(&write_target)?;
            let aad = format::write_header(&mut file, &header)?;
            let encryptor =
                crypto::EncryptingWriter::new(file, &file_key, &stream_nonce, &aad, CHUNK_SIZE)?;

            let mut progress = |event: ProgressEvent| match event {
                ProgressEvent::Entry {
                    path,
                    bytes_done,
                    bytes_total,
                } => debug!(entry = %path, bytes_done, bytes_total, "packing archive entry"),
                other => debug!(?other, "archive progress"),
            };

            let (manifest, encryptor) = snapshot::write_payload(encryptor, &inputs, &mut progress)?;
            let file = encryptor.finish()?;
            // The point of the whole exercise is surviving a crash, so the
            // bytes must be on the platter before the name says "complete".
            file.sync_all()?;
            drop(file);
            debug!(
                files = manifest.files_count,
                vault_files = manifest.vault_file_count,
                plaintext_bytes = manifest.total_plaintext_bytes,
                "archive payload written"
            );
            Ok(std::fs::metadata(&write_target)?.len())
        })
        .await
        .map_err(|e| ArchiveError::Other(format!("archive writer task failed: {e}")))?;

        let size = match written {
            Ok(size) => size,
            Err(e) => {
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(e);
            }
        };

        tokio::fs::rename(&part_path, &final_path).await?;
        info!(path = %final_path.display(), size, "encrypted backup archive written");

        apply_retention(&out_dir, config.keep_count).await;

        Ok(ArchiveRun {
            path: final_path,
            created_at,
            size,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }

    /// Resolve what actually gets packed. Everything is optional except the
    /// snapshot: a fresh install with no vault and no imported files still
    /// produces a valid archive.
    async fn collect_inputs(
        &self,
        report: &snapshot::SnapshotReport,
        out_dir: &Path,
        created_at: &chrono::DateTime<chrono::Utc>,
    ) -> Result<ArchiveInputs, crate::shared::error::AppError> {
        let settings = self.settings_repo.get_all().await?;

        let files_root = self.files_root.clone().filter(|root| path_exists(root));

        let vault_root = if settings.vault.enabled {
            crate::features::vault::writeback::resolve_vault_root(&settings.vault.vault_path)
                .filter(|root| path_exists(root))
        } else {
            None
        };

        let settings_file =
            Some(self.app_data_dir.join("settings.json")).filter(|p| path_exists(p));

        Ok(ArchiveInputs {
            db_snapshot: report.path.clone(),
            migration_version: report.migration_version,
            files_root,
            vault_root,
            settings_file,
            // If the user pointed the backup destination at a folder inside
            // their vault, packing the vault would pack the growing archive
            // into itself.
            exclude: vec![out_dir.to_path_buf()],
            app_version: APP_VERSION.to_string(),
            created_at: created_at.to_rfc3339(),
        })
    }
}

fn with_part_suffix(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(PART_SUFFIX);
    PathBuf::from(name)
}

fn build_header(
    created_at: &chrono::DateTime<chrono::Utc>,
    config: &ArchiveConfig,
) -> Result<ArchiveHeader, ArchiveError> {
    let file_salt = crypto::random_bytes::<FILE_SALT_LEN>();
    let stream_nonce = crypto::random_bytes::<STREAM_NONCE_LEN>();
    Ok(ArchiveHeader {
        version: FORMAT_VERSION,
        created_at: created_at.to_rfc3339(),
        app_version: APP_VERSION.to_string(),
        cipher: CIPHER_ID.to_string(),
        compression: COMPRESSION_ID.to_string(),
        chunk_size: u32::try_from(CHUNK_SIZE)
            .map_err(|_| ArchiveError::Other("chunk size does not fit in u32".to_string()))?,
        file_salt: BASE64_STANDARD.encode(file_salt),
        stream_nonce: BASE64_STANDARD.encode(stream_nonce),
        envelope: config.envelope.clone(),
    })
}

/// Delete all but the newest `keep_count` archives.
///
/// Only files whose name `format::is_archive_file_name` accepts are ever
/// considered. The destination is a folder the user picked — very possibly
/// the root of their Dropbox — so anything we did not write is untouchable,
/// and so are `.part` files belonging to a run in flight.
pub async fn apply_retention(out_dir: &Path, keep_count: u32) {
    let dir = out_dir.to_path_buf();
    let names = match tokio::task::spawn_blocking(move || archive_names(&dir)).await {
        Ok(names) => names,
        Err(e) => {
            warn!(error = %e, "retention scan failed");
            return;
        }
    };

    let keep = keep_count as usize;
    if names.len() <= keep {
        return;
    }

    for name in names.into_iter().skip(keep) {
        let path = out_dir.join(&name);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => info!(path = %path.display(), keep_count, "removed archive beyond retention"),
            Err(e) => warn!(error = %e, path = %path.display(), "could not remove old archive"),
        }
    }
}

/// Archive file names in `dir`, newest first. Non-archive files are invisible.
fn archive_names(dir: &Path) -> Vec<String> {
    // repository-barrier-allow: enumerating the user's own backup folder.
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            if !format::is_archive_file_name(&name) {
                return None;
            }
            // repository-barrier-allow: distinguishing files from directories
            // in the user's backup folder.
            entry.file_type().ok().filter(|t| t.is_file())?;
            Some(name)
        })
        .collect();
    names.sort_by(|a, b| b.cmp(a));
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    fn stamped(n: u32) -> String {
        format!("lattice-backup-2026091{n}T000000Z.lattice-backup")
    }

    #[tokio::test]
    async fn retention_keeps_the_newest_n() {
        let dir = tempfile::tempdir().unwrap();
        for n in 1..=5 {
            touch(dir.path(), &stamped(n));
        }

        apply_retention(dir.path(), 2).await;

        let mut left = archive_names(dir.path());
        left.sort();
        assert_eq!(left, vec![stamped(4), stamped(5)]);
    }

    #[tokio::test]
    async fn retention_never_touches_files_we_did_not_write() {
        let dir = tempfile::tempdir().unwrap();
        for n in 1..=4 {
            touch(dir.path(), &stamped(n));
        }
        // A user's own files, and an in-flight write, sharing the folder.
        touch(dir.path(), "tax-return.pdf");
        touch(dir.path(), "notes.lattice-backup");
        touch(
            dir.path(),
            "lattice-backup-20250101T000000Z.lattice-backup.part",
        );

        apply_retention(dir.path(), 1).await;

        assert!(dir.path().join("tax-return.pdf").exists());
        assert!(dir.path().join("notes.lattice-backup").exists());
        assert!(dir
            .path()
            .join("lattice-backup-20250101T000000Z.lattice-backup.part")
            .exists());
        assert_eq!(archive_names(dir.path()), vec![stamped(4)]);
    }

    #[tokio::test]
    async fn retention_is_a_no_op_below_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), &stamped(1));
        touch(dir.path(), &stamped(2));

        apply_retention(dir.path(), 5).await;

        assert_eq!(archive_names(dir.path()).len(), 2);
    }

    #[tokio::test]
    async fn retention_survives_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        apply_retention(&dir.path().join("nope"), 3).await;
    }

    #[test]
    fn listing_is_newest_first_and_ignores_strangers() {
        let dest = tempfile::tempdir().unwrap();
        let out = archive_dir(dest.path());
        std::fs::create_dir_all(&out).unwrap();
        touch(&out, &stamped(1));
        touch(&out, &stamped(3));
        touch(&out, &stamped(2));
        touch(&out, "holiday.jpg");

        let listed = list_archives(dest.path());
        let names: Vec<&str> = listed.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec![stamped(3), stamped(2), stamped(1)]);
        assert!(listed.iter().all(|a| a.size == 1));
    }

    #[test]
    fn listing_an_unconfigured_destination_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_archives(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn part_suffix_is_appended_not_substituted() {
        let path = Path::new("/tmp/Lattice Backups/lattice-backup-20260916T120000Z.lattice-backup");
        assert_eq!(
            with_part_suffix(path).to_string_lossy(),
            "/tmp/Lattice Backups/lattice-backup-20260916T120000Z.lattice-backup.part"
        );
    }

    #[test]
    fn header_carries_a_fresh_salt_and_nonce_each_time() {
        let config = ArchiveConfig::default();
        let now = chrono::Utc::now();
        let a = build_header(&now, &config).unwrap();
        let b = build_header(&now, &config).unwrap();

        assert_ne!(a.file_salt, b.file_salt);
        assert_ne!(a.stream_nonce, b.stream_nonce);
        assert_eq!(
            BASE64_STANDARD.decode(&a.file_salt).unwrap().len(),
            FILE_SALT_LEN
        );
        assert_eq!(
            BASE64_STANDARD.decode(&a.stream_nonce).unwrap().len(),
            STREAM_NONCE_LEN
        );
        assert_eq!(a.cipher, CIPHER_ID);
        assert_eq!(a.chunk_size as usize, CHUNK_SIZE);
    }
}
