//! Reading a `.lattice-backup` back onto a machine.
//!
//! Restore is the dangerous half of the feature: it replaces the live
//! database wholesale. The order below is the design document's, and each
//! step is a gate — nothing is written anywhere until the archive has been
//! authenticated, extracted, and checked against this build's migrations.
//!
//! Merging rules are conservative on purpose. The files library is
//! content-addressed, so an existing blob is by definition the same blob and
//! is left alone. A vault that already has content is never written into;
//! the restored copy lands beside it. `settings.json` is only restored when
//! the machine has none.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::application::ports::SettingsRepositoryPort;

use super::config::ArchiveConfig;
use super::crypto::{self, MasterKey};
use super::format::{self, ArchiveError, STREAM_NONCE_LEN};
use super::key_store::MasterKeyStore;
use super::placeholder::{self, FileAvailability};
use super::snapshot::{self, ExtractedPayload, ProgressEvent};

use base64::prelude::{Engine as _, BASE64_STANDARD};

/// Marker file the app checks at startup to know derived vectors are gone.
pub const REEMBED_MARKER: &str = crate::shared::constants::REEMBED_MARKER_FILE;

/// How long to let a sync client fetch an evicted archive before giving up
/// and telling the user to download it themselves. Generous on purpose: a
/// multi-gigabyte archive over a domestic uplink is the normal case, and the
/// alternative is a "try again" button the user has to keep pressing.
pub const HYDRATION_TIMEOUT: Duration = Duration::from_secs(120);

/// What a restore attempt concluded.
#[derive(Debug, Clone)]
pub enum RestoreOutcome {
    Restored {
        restart_required: bool,
        reembed_required: bool,
        vault_restored_to: Option<PathBuf>,
        files_restored: u64,
    },
    /// No usable key on this device and no secret supplied. The UI prompts
    /// for a passphrase or the recovery code and calls again.
    NeedsSecret,
    /// A cloud sync client has the file listed but not downloaded. Naming the
    /// provider is the difference between "try again later" and "open Finder
    /// and click the little cloud".
    NotHydrated(String),
}

/// Restores from an archive file. One instance per app, built in the backup DI.
pub struct ArchiveRestorer {
    pool: SqlitePool,
    db_path: PathBuf,
    app_data_dir: PathBuf,
    settings_repo: Arc<dyn SettingsRepositoryPort>,
    key_store: Arc<MasterKeyStore>,
    /// Where restored blobs land. Injected for the same reason the writer
    /// injects it: a test must never write into `~/.lattice/files`.
    files_root: Option<PathBuf>,
}

impl ArchiveRestorer {
    pub fn new(
        pool: SqlitePool,
        db_path: PathBuf,
        app_data_dir: PathBuf,
        settings_repo: Arc<dyn SettingsRepositoryPort>,
        key_store: Arc<MasterKeyStore>,
        files_root: Option<PathBuf>,
    ) -> Self {
        Self {
            pool,
            db_path,
            app_data_dir,
            settings_repo,
            key_store,
            files_root,
        }
    }

    pub async fn restore(
        &self,
        source: &Path,
        secret: Option<&str>,
    ) -> Result<RestoreOutcome, ArchiveError> {
        // 1. Never *silently* block on a download. Opening a dataless
        //    placeholder would hang for as long as the sync client takes, with
        //    no way to report progress, so nudge it explicitly on a bounded
        //    clock and hand the decision back to the user if it does not land.
        match placeholder::file_availability(source) {
            FileAvailability::Placeholder { provider } => {
                // The bare product name: the UI wraps it in its own sentence.
                let name = provider
                    .map(|p| p.display_name())
                    .unwrap_or_else(|| placeholder::UNKNOWN_PROVIDER_LABEL.to_string());
                info!(
                    path = %source.display(),
                    provider = %name,
                    "backup file is not downloaded yet; asking the sync client for it"
                );
                if let Err(e) = wait_for_hydration_off_thread(source).await {
                    info!(error = %e, provider = %name, "backup file did not download in time");
                    return Ok(RestoreOutcome::NotHydrated(name));
                }
                info!(provider = %name, "backup file downloaded; continuing");
            }
            FileAvailability::Missing => {
                return Err(ArchiveError::Other(format!(
                    "no such backup file: {}",
                    source.display()
                )));
            }
            FileAvailability::Local | FileAvailability::Unknown => {}
        }

        // 2. Header first: it is plaintext, authenticated by every chunk, and
        //    carries the envelope we need to find a key.
        let (header, _aad) = read_header_from(source)?;

        // 3. A key already on this device beats asking the user for anything.
        let (master, from_secret) = match self.key_store.load()? {
            Some(key) if crypto::kcv_matches(&header.envelope, &key) => (key, false),
            _ => match secret {
                // WrongSecret propagates: "that is not the right passphrase"
                // is a different message from "needs a secret".
                Some(secret) => (crypto::unwrap_master_key(&header.envelope, secret)?, true),
                None => return Ok(RestoreOutcome::NeedsSecret),
            },
        };

        let scratch = ArchiveConfig::scratch_dir(&self.app_data_dir);
        tokio::fs::create_dir_all(&scratch).await?;

        let outcome = self
            .restore_from_scratch(source, &header, &master, &scratch)
            .await;

        if let Err(e) = tokio::fs::remove_dir_all(&scratch).await {
            warn!(error = %e, scratch = %scratch.display(), "could not remove restore scratch directory");
        }

        let outcome = outcome?;

        // 4. The secret worked and this device had nothing: adopt the key so
        //    scheduled backups can resume without another prompt.
        if from_secret {
            if let Err(e) = self.adopt_key(&master, &header.envelope).await {
                warn!(error = %e, "restore succeeded but the backup key could not be saved on this device");
            }
        }

        Ok(outcome)
    }

    async fn restore_from_scratch(
        &self,
        source: &Path,
        header: &format::ArchiveHeader,
        master: &MasterKey,
        scratch: &Path,
    ) -> Result<RestoreOutcome, ArchiveError> {
        let extracted = self.extract(source, header, master, scratch).await?;

        // Refuse a snapshot from a build that knows migrations this one does
        // not: its schema would be ahead of the code about to open it.
        let info = snapshot::validate_snapshot(&extracted.db_path).await?;
        let newest_local = self.newest_local_migration().await?;
        if let (Some(snapshot_version), Some(local_version)) =
            (info.migration_version, newest_local)
        {
            if snapshot_version > local_version {
                return Err(ArchiveError::Corrupt(format!(
                    "backup was made by a newer version of Lattice (schema {snapshot_version} > {local_version}); update Lattice and try again"
                )));
            }
        }
        info!(
            documents = info.document_count,
            conversations = info.conversation_count,
            migration_version = ?info.migration_version,
            "snapshot validated"
        );

        self.swap_database(&extracted.db_path).await?;

        let files_restored = self
            .merge_files_library(extracted.files_dir.as_deref())
            .await;
        let vault_restored_to = self.restore_vault(extracted.vault_dir.as_deref()).await?;
        self.restore_settings_file(extracted.settings_file.as_deref())
            .await;
        self.mark_reembed_required().await;

        Ok(RestoreOutcome::Restored {
            restart_required: true,
            reembed_required: true,
            vault_restored_to,
            files_restored,
        })
    }

    async fn extract(
        &self,
        source: &Path,
        header: &format::ArchiveHeader,
        master: &MasterKey,
        scratch: &Path,
    ) -> Result<ExtractedPayload, ArchiveError> {
        let file_salt = BASE64_STANDARD
            .decode(&header.file_salt)
            .map_err(|e| ArchiveError::Corrupt(format!("bad file salt: {e}")))?;
        let stream_nonce: [u8; STREAM_NONCE_LEN] = BASE64_STANDARD
            .decode(&header.stream_nonce)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| ArchiveError::Corrupt("bad stream nonce".to_string()))?;
        let file_key = crypto::derive_file_key(master, &file_salt);
        let chunk_size = header.chunk_size as usize;

        let source = source.to_path_buf();
        let dest = scratch.join("payload");
        tokio::task::spawn_blocking(move || {
            // Re-open and re-parse so the reader sits exactly at the first
            // chunk and the associated data is the bytes actually on disk.
            let mut file = std::fs::File::open(&source)?;
            let (_, aad) = format::read_header(&mut file)?;
            let reader = crypto::DecryptingReader::new(
                std::io::BufReader::new(file),
                &file_key,
                &stream_nonce,
                &aad,
                chunk_size,
            )?;
            let mut progress = |event: ProgressEvent| match event {
                ProgressEvent::Entry {
                    path,
                    bytes_done,
                    bytes_total,
                } => debug!(entry = %path, bytes_done, bytes_total, "extracting archive entry"),
                other => debug!(?other, "restore progress"),
            };
            snapshot::extract_payload(reader, &dest, &mut progress)
        })
        .await
        .map_err(|e| ArchiveError::Other(format!("archive reader task failed: {e}")))?
    }

    /// Highest applied migration in the *live* database — what this build's
    /// schema actually is, rather than what the binary claims.
    async fn newest_local_migration(&self) -> Result<Option<i64>, ArchiveError> {
        match sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&self.pool)
            .await
        {
            Ok(version) => Ok(version),
            Err(e) => {
                // A database with no migration table is a test fixture or a
                // pre-migration install; treating that as "no opinion" is
                // better than blocking a legitimate restore.
                warn!(error = %e, "could not read local migration version; skipping the newer-backup check");
                Ok(None)
            }
        }
    }

    /// Swap the snapshot in, following `BackupAdapter::restore_backup`
    /// exactly: safety copy, close the pool, copy to a temp name, rename,
    /// roll back if the rename fails.
    async fn swap_database(&self, snapshot_path: &Path) -> Result<(), ArchiveError> {
        let safety = self.db_path.with_extension("db.pre_restore");
        // repository-barrier-allow: the live database file is the resource.
        if self.db_path.exists() {
            tokio::fs::copy(&self.db_path, &safety).await?;
            info!(path = %safety.display(), "created safety copy of the live database");
        }

        info!("closing database connections before swapping in the restored database");
        self.pool.close().await;

        let temp_path = self.db_path.with_extension("db.restoring");
        if let Err(e) = tokio::fs::copy(snapshot_path, &temp_path).await {
            // repository-barrier-allow: rolling back the live database file.
            if safety.exists() && !self.db_path.exists() {
                let _ = tokio::fs::rename(&safety, &self.db_path).await;
                info!("rolled back to the original database");
            }
            return Err(ArchiveError::Io(e));
        }

        match tokio::fs::rename(&temp_path, &self.db_path).await {
            Ok(()) => {
                info!("database restored; a restart is required to reconnect");
                Ok(())
            }
            Err(e) => {
                // repository-barrier-allow: rolling back the live database file.
                if safety.exists() {
                    let _ = tokio::fs::rename(&safety, &self.db_path).await;
                    info!("rolled back to the original database");
                }
                let _ = tokio::fs::remove_file(&temp_path).await;
                Err(ArchiveError::Io(e))
            }
        }
    }

    /// Copy `files/<sha>/<name>` into the content-addressed library, skipping
    /// anything already there. Same digest, same bytes — copying again would
    /// only risk clobbering a good file with a partial read.
    async fn merge_files_library(&self, files_dir: Option<&Path>) -> u64 {
        let Some(files_dir) = files_dir else {
            return 0;
        };
        let Some(root) = self.files_root.clone() else {
            warn!("no files library root; skipping file restore");
            return 0;
        };

        let files_dir = files_dir.to_path_buf();
        let restored = tokio::task::spawn_blocking(move || copy_new_files(&files_dir, &root)).await;
        match restored {
            Ok(count) => {
                info!(count, "restored files into the content-addressed library");
                count
            }
            Err(e) => {
                warn!(error = %e, "file library restore task failed");
                0
            }
        }
    }

    /// Put the vault back where it belongs when that is unambiguous, and
    /// beside it when it is not. Overwriting a vault the user has been
    /// editing would be the one unrecoverable mistake this feature could make.
    async fn restore_vault(
        &self,
        vault_dir: Option<&Path>,
    ) -> Result<Option<PathBuf>, ArchiveError> {
        let Some(vault_dir) = vault_dir else {
            return Ok(None);
        };

        let settings = self
            .settings_repo
            .get_all()
            .await
            .map_err(|e| ArchiveError::Other(format!("could not read settings: {e}")))?;
        let Some(target) =
            crate::features::vault::writeback::resolve_vault_root(&settings.vault.vault_path)
        else {
            warn!("no vault path could be resolved; leaving the restored vault unplaced");
            return Ok(None);
        };

        let source = vault_dir.to_path_buf();
        let destination = target.clone();
        let placed = tokio::task::spawn_blocking(move || place_vault(&source, &destination))
            .await
            .map_err(|e| ArchiveError::Other(format!("vault restore task failed: {e}")))??;

        if placed == target {
            info!(path = %placed.display(), "vault restored in place");
            Ok(None)
        } else {
            warn!(
                path = %placed.display(),
                "existing vault was not empty; restored copy written beside it"
            );
            Ok(Some(placed))
        }
    }

    /// Only when this machine has none. A restored `settings.json` carries
    /// the *old* machine's model paths and indexed folders, which would be
    /// wrong here — but an empty install has nothing better.
    async fn restore_settings_file(&self, settings_file: Option<&Path>) {
        let Some(settings_file) = settings_file else {
            return;
        };
        let target = self.app_data_dir.join("settings.json");
        // repository-barrier-allow: settings.json is the file resource itself.
        if target.exists() {
            debug!("settings.json already exists; the archived copy was not restored");
            return;
        }
        match tokio::fs::copy(settings_file, &target).await {
            Ok(_) => info!(path = %target.display(), "restored settings.json"),
            Err(e) => warn!(error = %e, "could not restore settings.json"),
        }
    }

    /// Embeddings, clusters and sparse terms were stripped from the snapshot;
    /// the app has to rebuild them. The marker survives the restart that the
    /// database swap requires, which an in-memory flag would not.
    async fn mark_reembed_required(&self) {
        let marker = self.app_data_dir.join(REEMBED_MARKER);
        let body = chrono::Utc::now().to_rfc3339();
        match tokio::fs::write(&marker, body.as_bytes()).await {
            Ok(()) => info!(path = %marker.display(), "flagged the corpus for re-embedding"),
            Err(e) => warn!(error = %e, "could not write the re-embed marker"),
        }
    }

    /// After a secret-based restore on a new machine, keep the key and make
    /// sure an `archive-config.json` exists so scheduled archives can resume
    /// once the user picks a destination.
    async fn adopt_key(
        &self,
        master: &MasterKey,
        envelope: &format::KeyEnvelope,
    ) -> Result<(), ArchiveError> {
        self.key_store.store(master)?;

        if ArchiveConfig::load(&self.app_data_dir).await?.is_some() {
            return Ok(());
        }
        let config = ArchiveConfig {
            envelope: envelope.clone(),
            // The source machine's destination folder does not exist here.
            destination: None,
            ..ArchiveConfig::default()
        };
        config.save(&self.app_data_dir).await?;
        info!("adopted the backup key from the restored archive");
        Ok(())
    }
}

/// `placeholder::wait_for_hydration` takes a `&mut dyn FnMut` progress sink,
/// which makes the future it returns neither `Send` nor `Sync` — and a Tauri
/// command's future must be `Send`. Give it a thread and a runtime of its own
/// so the non-`Send` borrow never has to cross a thread boundary, and so the
/// wait cannot deadlock a single-threaded caller.
///
/// This runs at most once per restore, and only for a file a sync client has
/// evicted, so the extra runtime costs nothing that matters.
async fn wait_for_hydration_off_thread(source: &Path) -> Result<(), ArchiveError> {
    let source = source.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(ArchiveError::Io)?;
        runtime.block_on(async {
            let mut progress = |elapsed: Duration| {
                debug!(
                    seconds = elapsed.as_secs(),
                    "waiting for the sync client to download the backup"
                );
            };
            placeholder::wait_for_hydration(&source, HYDRATION_TIMEOUT, &mut progress).await
        })
    })
    .await
    .map_err(|e| ArchiveError::Other(format!("hydration wait task failed: {e}")))?
}

fn read_header_from(source: &Path) -> Result<(format::ArchiveHeader, Vec<u8>), ArchiveError> {
    let mut file = std::fs::File::open(source)?;
    format::read_header(&mut file)
}

/// `<extracted>/<sha>/<name>` -> `<root>/<sha>/<name>`, skipping existing.
fn copy_new_files(files_dir: &Path, root: &Path) -> u64 {
    let mut restored = 0u64;
    // repository-barrier-allow: walking the extracted payload on disk.
    let digests = match std::fs::read_dir(files_dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(error = %e, "could not read the extracted files directory");
            return 0;
        }
    };

    for digest_entry in digests.filter_map(|e| e.ok()) {
        let digest_dir = digest_entry.path();
        // repository-barrier-allow: walking the extracted payload on disk.
        let Ok(files) = std::fs::read_dir(&digest_dir) else {
            continue;
        };
        let Some(digest) = digest_entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let target_dir = root.join(&digest);

        for file_entry in files.filter_map(|e| e.ok()) {
            let source = file_entry.path();
            // repository-barrier-allow: classifying an extracted payload entry.
            if !source.is_file() {
                continue;
            }
            let target = target_dir.join(file_entry.file_name());
            // repository-barrier-allow: content-addressed skip — identical
            // digest means identical bytes.
            if target.exists() {
                continue;
            }
            if let Err(e) = std::fs::create_dir_all(&target_dir) {
                warn!(error = %e, path = %target_dir.display(), "could not create library folder");
                continue;
            }
            match std::fs::copy(&source, &target) {
                Ok(_) => restored += 1,
                Err(e) => warn!(error = %e, path = %target.display(), "could not restore file"),
            }
        }
    }
    restored
}

/// Move the extracted vault to `target`, or beside it when `target` already
/// holds something. Returns where it actually landed.
fn place_vault(source: &Path, target: &Path) -> Result<PathBuf, ArchiveError> {
    if is_free_for_vault(target) {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // An empty directory would make `rename` fail on some platforms.
        let _ = std::fs::remove_dir(target);
        move_dir(source, target)?;
        return Ok(target.to_path_buf());
    }

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let mut name = target.as_os_str().to_os_string();
    name.push(format!("-restored-{stamp}"));
    let beside = PathBuf::from(name);
    move_dir(source, &beside)?;
    Ok(beside)
}

/// True when writing the vault here loses nothing: no such path, or an
/// empty directory.
fn is_free_for_vault(target: &Path) -> bool {
    // repository-barrier-allow: the vault folder is the resource being placed.
    match std::fs::read_dir(target) {
        Ok(mut entries) => entries.next().is_none(),
        // Not a directory, or missing. Missing is free; a *file* there is not.
        // repository-barrier-allow: the vault folder is the resource being placed.
        Err(_) => !target.exists(),
    }
}

/// `rename` where possible, recursive copy across filesystems (the scratch
/// directory and the user's home are often different volumes).
fn move_dir(source: &Path, target: &Path) -> Result<(), ArchiveError> {
    if std::fs::rename(source, target).is_ok() {
        return Ok(());
    }
    copy_dir_recursive(source, target)?;
    if let Err(e) = std::fs::remove_dir_all(source) {
        debug!(error = %e, "could not clean up the extracted vault copy");
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), ArchiveError> {
    std::fs::create_dir_all(target)?;
    // repository-barrier-allow: walking the extracted payload on disk.
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn files_merge_skips_digests_already_present() {
        let dir = tempfile::tempdir().unwrap();
        let extracted = dir.path().join("files");
        let root = dir.path().join("library");

        write(&extracted.join("aaa").join("one.md"), "new");
        write(&extracted.join("bbb").join("two.md"), "also new");
        // Already in the library, with different bytes: must not be replaced.
        write(&root.join("aaa").join("one.md"), "existing");

        let restored = copy_new_files(&extracted, &root);

        assert_eq!(restored, 1);
        assert_eq!(
            std::fs::read_to_string(root.join("aaa").join("one.md")).unwrap(),
            "existing"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("bbb").join("two.md")).unwrap(),
            "also new"
        );
    }

    #[test]
    fn files_merge_tolerates_a_missing_source() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(copy_new_files(&dir.path().join("nope"), dir.path()), 0);
    }

    #[test]
    fn vault_lands_in_place_when_the_target_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("extracted-vault");
        write(&source.join("note.md"), "hello");
        let target = dir.path().join("Lattice");

        let placed = place_vault(&source, &target).unwrap();

        assert_eq!(placed, target);
        assert_eq!(
            std::fs::read_to_string(target.join("note.md")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn vault_lands_in_place_when_the_target_is_an_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("extracted-vault");
        write(&source.join("note.md"), "hello");
        let target = dir.path().join("Lattice");
        std::fs::create_dir_all(&target).unwrap();

        assert_eq!(place_vault(&source, &target).unwrap(), target);
        assert!(target.join("note.md").exists());
    }

    #[test]
    fn a_vault_with_content_is_never_written_into() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("extracted-vault");
        write(&source.join("note.md"), "from backup");
        let target = dir.path().join("Lattice");
        write(&target.join("mine.md"), "my work");

        let placed = place_vault(&source, &target).unwrap();

        assert_ne!(placed, target);
        assert!(placed
            .to_string_lossy()
            .contains(&format!("{}-restored-", target.display())));
        assert_eq!(
            std::fs::read_to_string(target.join("mine.md")).unwrap(),
            "my work"
        );
        assert!(!target.join("note.md").exists());
        assert_eq!(
            std::fs::read_to_string(placed.join("note.md")).unwrap(),
            "from backup"
        );
    }

    #[test]
    fn move_dir_falls_back_to_a_recursive_copy() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        write(&source.join("a").join("b.md"), "deep");
        let target = dir.path().join("dst");

        copy_dir_recursive(&source, &target).unwrap();

        assert_eq!(
            std::fs::read_to_string(target.join("a").join("b.md")).unwrap(),
            "deep"
        );
    }
}
