//! The one object the command layer talks to.
//!
//! It owns the writer, the restorer, the key store and the transient "pending
//! setup" state, and turns all of that into the DTOs the settings panel
//! renders. Filesystem and database work lives in `config`, `writer` and
//! `restore`; this module only sequences and validates.
//!
//! Setup is two-phase on purpose. `begin_setup` generates a master key and a
//! 24-word recovery code and keeps them **in memory only**; nothing reaches
//! the disk until `confirm_setup` proves the user actually wrote the words
//! down. A recovery code that was never transcribed is worse than none — it
//! creates the belief in a way back that does not exist.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::info;

use crate::features::backup::dto::{
    archive_availability, ArchiveErrorDto, ArchiveFileDto, ArchiveRunDto, ArchiveSetupDto,
    ArchiveStatusDto, WordConfirmationDto,
};
use crate::shared::error::{AppError, Result};

use super::config::{self, ArchiveConfig, DEFAULT_KEEP_COUNT};
use super::crypto::{self, Argon2Params, MasterKey, RecoveryCode};
use super::format::ArchiveError;
use super::key_store::MasterKeyStore;
use super::placeholder::{self, FileAvailability};
use super::restore::{ArchiveRestorer, RestoreOutcome};
use super::writer::{self, ArchiveRun, ArchiveWriter};

/// Words in a BIP-39 256-bit recovery code.
pub const RECOVERY_WORD_COUNT: usize = 24;
/// How many of them the user retypes before anything is persisted.
pub const CONFIRM_WORD_COUNT: usize = 3;
/// Shortest passphrase accepted for the optional second slot.
pub const MIN_PASSPHRASE_LEN: usize = 8;

/// A generated-but-unconfirmed setup. Dropped, and with it the key, if the
/// user closes the wizard.
struct PendingSetup {
    master: MasterKey,
    recovery: RecoveryCode,
    confirm_indices: [usize; CONFIRM_WORD_COUNT],
    /// True when this is a recovery-code rotation: the master key is the
    /// existing one and the passphrase slot must survive untouched.
    rotate_only: bool,
}

pub struct ArchiveService {
    writer: Arc<ArchiveWriter>,
    restorer: Arc<ArchiveRestorer>,
    key_store: Arc<MasterKeyStore>,
    app_data_dir: PathBuf,
    /// The folder holding `lattice.db`. Separate from `app_data_dir` because
    /// the cloud-corruption warning follows the *database file*: SQLite is
    /// what a sync client tears apart, and it is the database's own directory
    /// that has to be outside one.
    db_dir: PathBuf,
    pending: Mutex<Option<PendingSetup>>,
    /// The archive the user picked in the native dialog, kept across the
    /// `needs_secret` round trip so answering a passphrase prompt does not
    /// pop the file picker at them a second time. Cleared once the restore
    /// actually completes.
    restore_source: Mutex<Option<PathBuf>>,
}

impl ArchiveService {
    pub fn new(
        writer: Arc<ArchiveWriter>,
        restorer: Arc<ArchiveRestorer>,
        key_store: Arc<MasterKeyStore>,
        app_data_dir: PathBuf,
        db_dir: PathBuf,
    ) -> Self {
        Self {
            writer,
            restorer,
            key_store,
            app_data_dir,
            db_dir,
            pending: Mutex::new(None),
            restore_source: Mutex::new(None),
        }
    }

    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    /// True when a scheduled run would actually produce something.
    pub async fn is_active(&self) -> bool {
        match ArchiveConfig::load(&self.app_data_dir).await {
            Ok(Some(config)) => config.is_active(),
            Ok(None) => false,
            Err(e) => {
                tracing::warn!(error = %e, "could not read the archive config");
                false
            }
        }
    }

    // -- status ------------------------------------------------------------

    pub async fn status(&self) -> Result<ArchiveStatusDto> {
        let data_dir_cloud_provider =
            placeholder::classify_cloud_location(&self.db_dir).map(|p| p.display_name());

        let Some(config) = ArchiveConfig::load(&self.app_data_dir).await? else {
            return Ok(ArchiveStatusDto {
                configured: false,
                destination: None,
                destination_provider: None,
                destination_missing: false,
                keep_count: DEFAULT_KEEP_COUNT,
                has_passphrase: false,
                recovery_confirmed: false,
                last_success: None,
                last_error: None,
                data_dir_cloud_provider,
                archives: Vec::new(),
            });
        };

        let (destination, destination_provider, destination_missing, archives) =
            match config.destination.clone() {
                Some(dest) => {
                    let (missing, archives) = self.inspect_destination(dest.clone()).await;
                    (
                        Some(dest.to_string_lossy().to_string()),
                        placeholder::classify_cloud_location(&dest).map(|p| p.display_name()),
                        missing,
                        archives,
                    )
                }
                None => (None, None, false, Vec::new()),
            };

        Ok(ArchiveStatusDto {
            configured: config.is_configured(),
            destination,
            destination_provider,
            destination_missing,
            keep_count: config.keep_count,
            has_passphrase: config.envelope.has_passphrase_slot(),
            recovery_confirmed: config.recovery_confirmed_at.is_some(),
            last_success: config.last_success.as_ref().map(|r| ArchiveRunDto {
                path: r.path.clone(),
                created_at: r.created_at.clone(),
                size: r.size,
                duration_ms: r.duration_ms,
            }),
            last_error: config.last_error.as_ref().map(|e| ArchiveErrorDto {
                at: e.at.clone(),
                message: e.message.clone(),
            }),
            data_dir_cloud_provider,
            archives,
        })
    }

    /// Stat the destination off the async runtime: a sleeping network volume
    /// can make a directory listing take seconds.
    async fn inspect_destination(&self, destination: PathBuf) -> (bool, Vec<ArchiveFileDto>) {
        let listed = tokio::task::spawn_blocking(move || {
            if !writer::path_exists(&destination) {
                return (true, Vec::new());
            }
            (false, writer::list_archives(&destination))
        })
        .await;

        match listed {
            Ok((missing, archives)) => (
                missing,
                archives.into_iter().map(archive_file_dto).collect(),
            ),
            Err(e) => {
                tracing::warn!(error = %e, "could not inspect the backup destination");
                (false, Vec::new())
            }
        }
    }

    // -- setup -------------------------------------------------------------

    /// Generate a key and a recovery code, held in memory until confirmed.
    /// Calling again discards the previous pending state.
    pub async fn begin_setup(&self) -> Result<ArchiveSetupDto> {
        let master = MasterKey::generate();
        let recovery = RecoveryCode::generate().map_err(AppError::from)?;
        self.stage(master, recovery, false).await
    }

    /// A fresh recovery code for an existing key. The passphrase slot, and
    /// every archive already written, keep working.
    pub async fn rotate_recovery_code(&self) -> Result<ArchiveSetupDto> {
        let config = self.require_config().await?;
        let master = self.require_master(&config).await?;
        let recovery = RecoveryCode::generate().map_err(AppError::from)?;
        self.stage(master, recovery, true).await
    }

    async fn stage(
        &self,
        master: MasterKey,
        recovery: RecoveryCode,
        rotate_only: bool,
    ) -> Result<ArchiveSetupDto> {
        let words = recovery.words().to_vec();
        if words.len() != RECOVERY_WORD_COUNT {
            return Err(AppError::InternalError(format!(
                "recovery code has {} words, expected {RECOVERY_WORD_COUNT}",
                words.len()
            )));
        }
        let confirm_indices = pick_confirm_indices();

        *self.pending.lock().await = Some(PendingSetup {
            master,
            recovery,
            confirm_indices,
            rotate_only,
        });

        Ok(ArchiveSetupDto {
            recovery_words: words,
            confirm_indices: confirm_indices
                .iter()
                .map(|i| u32::try_from(*i).unwrap_or(0))
                .collect(),
        })
    }

    /// Check the three words and, only then, persist the envelope and key.
    ///
    /// A wrong answer leaves the pending state in place so the user can try
    /// again without regenerating (and re-transcribing) the whole code.
    pub async fn confirm_setup(
        &self,
        confirmations: Vec<WordConfirmationDto>,
        passphrase: Option<String>,
    ) -> Result<ArchiveStatusDto> {
        let passphrase = normalize_passphrase(passphrase)?;

        let mut guard = self.pending.lock().await;
        let pending = guard.as_ref().ok_or_else(|| {
            AppError::InvalidState(
                "start the backup setup again — the recovery code is no longer in memory"
                    .to_string(),
            )
        })?;

        verify_confirmations(
            pending.recovery.words(),
            &pending.confirm_indices,
            &confirmations,
        )?;

        let mut config = ArchiveConfig::load(&self.app_data_dir)
            .await?
            .unwrap_or_default();

        if pending.rotate_only {
            crypto::rotate_recovery_slot(&mut config.envelope, &pending.master, &pending.recovery)
                .map_err(AppError::from)?;
            info!("recovery code rotated");
        } else {
            config.envelope = crypto::build_envelope(
                &pending.master,
                passphrase.as_deref(),
                &pending.recovery,
                Argon2Params::default(),
            )
            .map_err(AppError::from)?;
            self.key_store.store(&pending.master)?;
            info!(
                has_passphrase = passphrase.is_some(),
                "off-device backup key envelope created"
            );
        }
        config.recovery_confirmed_at = Some(chrono::Utc::now().to_rfc3339());
        config.save(&self.app_data_dir).await?;

        *guard = None;
        drop(guard);

        self.status().await
    }

    // -- configuration -----------------------------------------------------

    pub async fn set_destination(&self, destination: PathBuf) -> Result<ArchiveStatusDto> {
        config::validate_destination(&destination, &self.app_data_dir)
            .map_err(|e| AppError::InvalidInput(e.to_string()))?;

        let mut config = self.require_config().await?;
        config.destination = Some(destination.clone());
        config.save(&self.app_data_dir).await?;
        info!(destination = %destination.display(), "off-device backup destination set");

        self.status().await
    }

    pub async fn set_keep_count(&self, keep_count: u32) -> Result<ArchiveStatusDto> {
        let keep_count = config::validate_keep_count(keep_count)
            .map_err(|e| AppError::InvalidInput(e.to_string()))?;

        let mut config = self.require_config().await?;
        config.keep_count = keep_count;
        config.save(&self.app_data_dir).await?;

        self.status().await
    }

    /// Add, replace or remove the passphrase slot. Needs the master key, so
    /// it only works on a device that has one.
    pub async fn set_passphrase(&self, passphrase: Option<String>) -> Result<ArchiveStatusDto> {
        let passphrase = normalize_passphrase(passphrase)?;
        let mut config = self.require_config().await?;

        match passphrase {
            Some(passphrase) => {
                let master = self.require_master(&config).await?;
                crypto::set_passphrase_slot(
                    &mut config.envelope,
                    &master,
                    &passphrase,
                    Argon2Params::default(),
                )
                .map_err(AppError::from)?;
                info!("backup passphrase set");
            }
            None => {
                crypto::remove_passphrase_slot(&mut config.envelope);
                info!("backup passphrase removed; the recovery code still works");
            }
        }
        config.save(&self.app_data_dir).await?;

        self.status().await
    }

    /// Stop writing archives. The key and envelope stay so the user can turn
    /// it back on — and so old archives stay restorable from this device.
    pub async fn disable(&self) -> Result<ArchiveStatusDto> {
        let mut config = self.require_config().await?;
        config.destination = None;
        config.save(&self.app_data_dir).await?;
        info!("off-device backup turned off; key and envelope kept");

        self.status().await
    }

    // -- running -----------------------------------------------------------

    /// `ArchiveError` rather than `AppError` so the command layer can still
    /// tell a wrong passphrase from a corrupt file after the fact.
    pub async fn create_now(&self) -> std::result::Result<ArchiveRun, ArchiveError> {
        self.writer.create().await
    }

    pub async fn restore(
        &self,
        source: &Path,
        secret: Option<&str>,
    ) -> std::result::Result<RestoreOutcome, ArchiveError> {
        self.restorer.restore(source, secret).await
    }

    // -- restore source memory ---------------------------------------------

    /// The file the user already chose, if a restore is mid-conversation.
    pub async fn remembered_restore_source(&self) -> Option<PathBuf> {
        self.restore_source.lock().await.clone()
    }

    pub async fn remember_restore_source(&self, source: PathBuf) {
        *self.restore_source.lock().await = Some(source);
    }

    pub async fn forget_restore_source(&self) {
        *self.restore_source.lock().await = None;
    }

    // -- helpers -----------------------------------------------------------

    async fn require_config(&self) -> Result<ArchiveConfig> {
        ArchiveConfig::load(&self.app_data_dir)
            .await?
            .filter(ArchiveConfig::is_configured)
            .ok_or_else(|| AppError::InvalidState("set up off-device backup first".to_string()))
    }

    async fn require_master(&self, config: &ArchiveConfig) -> Result<MasterKey> {
        self.key_store
            .load()?
            .filter(|key| crypto::kcv_matches(&config.envelope, key))
            .ok_or_else(|| AppError::InvalidState(writer::MISSING_KEY_MESSAGE.to_string()))
    }
}

fn archive_file_dto(info: writer::ArchiveFileInfo) -> ArchiveFileDto {
    ArchiveFileDto {
        path: info.path.to_string_lossy().to_string(),
        name: info.name,
        created_at: info.created_at,
        size: info.size,
        availability: match info.availability {
            FileAvailability::Local => archive_availability::LOCAL,
            FileAvailability::Placeholder { .. } => archive_availability::PLACEHOLDER,
            FileAvailability::Missing | FileAvailability::Unknown => archive_availability::UNKNOWN,
        }
        .to_string(),
    }
}

/// Three distinct positions in the recovery code, in ascending order so the
/// wizard can show them as a natural checklist.
fn pick_confirm_indices() -> [usize; CONFIRM_WORD_COUNT] {
    use rand::seq::SliceRandom;

    let mut all: Vec<usize> = (0..RECOVERY_WORD_COUNT).collect();
    all.shuffle(&mut rand::thread_rng());
    all.truncate(CONFIRM_WORD_COUNT);
    all.sort_unstable();

    let mut picked = [0usize; CONFIRM_WORD_COUNT];
    for (slot, value) in picked.iter_mut().zip(all) {
        *slot = value;
    }
    picked
}

/// An empty or whitespace-only passphrase is "no passphrase", not a
/// one-character one.
fn normalize_passphrase(passphrase: Option<String>) -> Result<Option<String>> {
    let Some(passphrase) = passphrase else {
        return Ok(None);
    };
    if passphrase.trim().is_empty() {
        return Ok(None);
    }
    if passphrase.chars().count() < MIN_PASSPHRASE_LEN {
        return Err(AppError::InvalidInput(format!(
            "passphrase must be at least {MIN_PASSPHRASE_LEN} characters"
        )));
    }
    Ok(Some(passphrase))
}

/// Case- and whitespace-insensitive comparison of the requested words.
///
/// Every requested index must be answered and every answer must match; an
/// answer for a word we did not ask about is a sign the client is confused
/// rather than the user being right.
fn verify_confirmations(
    words: &[String],
    indices: &[usize],
    confirmations: &[WordConfirmationDto],
) -> Result<()> {
    if confirmations.len() != indices.len() {
        return Err(AppError::InvalidInput(format!(
            "confirm {} words from your recovery code",
            indices.len()
        )));
    }

    for index in indices {
        let expected = words.get(*index).ok_or_else(|| {
            AppError::InternalError("recovery code is shorter than expected".to_string())
        })?;
        let answer = confirmations
            .iter()
            .find(|c| usize::try_from(c.index).ok() == Some(*index))
            .ok_or_else(|| {
                AppError::InvalidInput(format!("word {} is missing", index.saturating_add(1)))
            })?;

        if !answer.word.trim().eq_ignore_ascii_case(expected.trim()) {
            return Err(AppError::InvalidInput(format!(
                "word {} does not match your recovery code",
                index.saturating_add(1)
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words() -> Vec<String> {
        [
            "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract",
            "absurd", "abuse", "access", "accident", "account", "accuse", "achieve", "acid",
            "acoustic", "acquire", "across", "act", "action", "actor", "actress", "actual",
        ]
        .iter()
        .map(|w| w.to_string())
        .collect()
    }

    fn answer(index: usize, word: &str) -> WordConfirmationDto {
        WordConfirmationDto {
            index: index as u32,
            word: word.to_string(),
        }
    }

    #[test]
    fn right_words_are_accepted() {
        let indices = [0usize, 5, 23];
        let given = vec![
            answer(0, "abandon"),
            answer(5, "absent"),
            answer(23, "actual"),
        ];
        assert!(verify_confirmations(&words(), &indices, &given).is_ok());
    }

    #[test]
    fn case_and_padding_are_forgiven() {
        let indices = [0usize, 5, 23];
        let given = vec![
            answer(0, "  ABANDON "),
            answer(5, "Absent"),
            answer(23, "aCtUaL"),
        ];
        assert!(verify_confirmations(&words(), &indices, &given).is_ok());
    }

    #[test]
    fn a_wrong_word_is_rejected() {
        let indices = [0usize, 5, 23];
        let given = vec![
            answer(0, "abandon"),
            answer(5, "absurd"), // a real BIP-39 word, but not word 6
            answer(23, "actual"),
        ];
        let err = verify_confirmations(&words(), &indices, &given).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
        assert!(err.to_string().contains("word 6"));
    }

    #[test]
    fn a_missing_answer_is_rejected() {
        let indices = [0usize, 5, 23];
        let given = vec![answer(0, "abandon"), answer(5, "absent")];
        assert!(verify_confirmations(&words(), &indices, &given).is_err());
    }

    #[test]
    fn answers_for_words_we_did_not_ask_about_are_rejected() {
        let indices = [0usize, 5, 23];
        let given = vec![
            answer(0, "abandon"),
            answer(5, "absent"),
            answer(7, "abstract"),
        ];
        assert!(verify_confirmations(&words(), &indices, &given).is_err());
    }

    #[test]
    fn confirm_indices_are_three_distinct_positions_in_range() {
        for _ in 0..200 {
            let picked = pick_confirm_indices();
            assert!(picked.iter().all(|i| *i < RECOVERY_WORD_COUNT));
            assert!(picked[0] < picked[1] && picked[1] < picked[2]);
        }
    }

    #[test]
    fn passphrase_length_is_enforced_and_blank_means_none() {
        assert_eq!(normalize_passphrase(None).unwrap(), None);
        assert_eq!(normalize_passphrase(Some("   ".into())).unwrap(), None);
        assert!(normalize_passphrase(Some("short".into())).is_err());
        assert_eq!(
            normalize_passphrase(Some("longenough".into())).unwrap(),
            Some("longenough".to_string())
        );
    }
    // -- end to end --------------------------------------------------------

    use crate::application::ports::settings_port::MockSettingsRepository;
    use crate::application::ports::SettingsRepositoryPort;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;

    /// The blob every seeded document points at: it is in the archive.
    const REFERENCED_BLOB: &str =
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    /// A blob no document points at: an import that never committed. It is
    /// on disk, and it is not in the archive.
    const ORPHAN_BLOB: &str = "5feceb66ffc86f38d952786c6d696c79c2dbc239dd4e91b46729d73a27fb57e9";

    /// Open a read-write pool on a fixture file.
    ///
    /// Built from `SqliteConnectOptions` rather than a `sqlite://…` string for
    /// the reason `snapshot.rs` and `adapter.rs` do: a DSN is a URL and a
    /// Windows path is not a URL component, so a drive letter becomes the
    /// authority and a `\\?\` verbatim prefix truncates the filename at its
    /// `?`. Passing the `Path` through sidesteps the whole question.
    async fn connect_fixture_pool(db_path: &Path) -> SqlitePool {
        SqlitePoolOptions::new()
            .connect_with(SqliteConnectOptions::new().filename(db_path))
            .await
            .unwrap()
    }

    /// A file-backed pool with one row in it, built the way `adapter.rs`
    /// builds its fixtures.
    async fn seeded_pool(db_path: &Path, rows: u32) -> SqlitePool {
        std::fs::File::create(db_path).unwrap();
        let pool = connect_fixture_pool(db_path).await;
        sqlx::query(
            "CREATE TABLE documents (id TEXT PRIMARY KEY, title TEXT NOT NULL, checksum TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for n in 0..rows {
            sqlx::query("INSERT INTO documents (id, title, checksum) VALUES (?, ?, ?)")
                .bind(n.to_string())
                .bind(format!("Doc {n}"))
                // Every document shares the one referenced blob, which is
                // what the library's deduplication would produce anyway.
                .bind(REFERENCED_BLOB)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    async fn document_count(db_path: &Path) -> i64 {
        let pool = connect_fixture_pool(db_path).await;
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        count.0
    }

    /// Never the real keyring and never `~/.lattice/files`: this test writes
    /// and restores real files, and it must only ever touch its own tempdirs.
    fn isolated_key_store(app_data_dir: &Path) -> Arc<MasterKeyStore> {
        Arc::new(MasterKeyStore::with_service(
            "tech.lattice.test.archive-round-trip".to_string(),
            app_data_dir.to_path_buf(),
            false,
        ))
    }

    /// The whole feature in one pass: set up, write an archive, then restore
    /// it on a machine that has never seen the key, using only the recovery
    /// code. This is the promise the feature makes to the user.
    #[tokio::test]
    async fn archive_round_trip_restores_database() {
        // --- source machine ---
        let source = tempfile::tempdir().unwrap();
        let app_data = source.path().join("app-data");
        let files_root = source.path().join("files-library");
        std::fs::create_dir_all(&app_data).unwrap();
        // A library as it really looks: the blob the documents reference, an
        // orphan from an import that died, and something that is not a blob
        // at all. Only the first is the user's data.
        std::fs::create_dir_all(files_root.join(REFERENCED_BLOB)).unwrap();
        std::fs::write(
            files_root.join(REFERENCED_BLOB).join("paper.pdf"),
            b"pdf bytes",
        )
        .unwrap();
        std::fs::create_dir_all(files_root.join(ORPHAN_BLOB)).unwrap();
        std::fs::write(
            files_root.join(ORPHAN_BLOB).join("ghost.pdf"),
            b"nothing points at this",
        )
        .unwrap();
        std::fs::write(files_root.join("stray.txt"), b"not a blob").unwrap();

        let db_path = app_data.join("lattice.db");
        let pool = seeded_pool(&db_path, 3).await;

        // Outside app_data, as `validate_destination` insists.
        let destination = tempfile::tempdir().unwrap();

        let settings: Arc<dyn SettingsRepositoryPort> = Arc::new(MockSettingsRepository::new());
        let key_store = isolated_key_store(&app_data);
        let service = ArchiveService::new(
            Arc::new(ArchiveWriter::new(
                pool.clone(),
                app_data.clone(),
                settings.clone(),
                key_store.clone(),
                Some(files_root.clone()),
            )),
            Arc::new(ArchiveRestorer::new(
                pool.clone(),
                db_path.clone(),
                app_data.clone(),
                settings.clone(),
                key_store.clone(),
                Some(files_root.clone()),
            )),
            key_store,
            app_data.clone(),
            app_data.clone(),
        );

        // Nothing configured yet.
        let status = service.status().await.unwrap();
        assert!(!status.configured);
        assert!(status.archives.is_empty());
        assert_eq!(status.keep_count, DEFAULT_KEEP_COUNT);

        // Setup: generate, confirm the three words, choose a folder.
        let setup = service.begin_setup().await.unwrap();
        assert_eq!(setup.recovery_words.len(), RECOVERY_WORD_COUNT);
        assert_eq!(setup.confirm_indices.len(), CONFIRM_WORD_COUNT);

        let confirmations: Vec<WordConfirmationDto> = setup
            .confirm_indices
            .iter()
            .map(|index| WordConfirmationDto {
                index: *index,
                // Upper case on purpose: the wizard must forgive it.
                word: setup.recovery_words[*index as usize].to_uppercase(),
            })
            .collect();
        let status = service.confirm_setup(confirmations, None).await.unwrap();
        assert!(status.configured);
        assert!(status.recovery_confirmed);
        assert!(!status.has_passphrase);

        // The frontend sets the passphrase as a separate step afterwards.
        let status = service
            .set_passphrase(Some("correct horse battery".to_string()))
            .await
            .unwrap();
        assert!(status.has_passphrase);

        let status = service
            .set_destination(destination.path().to_path_buf())
            .await
            .unwrap();
        assert!(!status.destination_missing);

        // --- write one ---
        let run = service.create_now().await.unwrap();
        assert!(run.size > 0, "archive should not be empty");
        assert!(super::super::format::is_archive_file_name(
            run.path.file_name().unwrap().to_str().unwrap()
        ));

        let status = service.status().await.unwrap();
        assert_eq!(status.archives.len(), 1);
        assert!(status.last_error.is_none());
        assert_eq!(status.last_success.as_ref().map(|s| s.size), Some(run.size));

        // The scratch directory never survives a run.
        assert!(!app_data.join("backup").join("scratch").join("x").exists());

        // --- fresh machine: no key, no config, empty database ---
        let target = tempfile::tempdir().unwrap();
        let target_app_data = target.path().join("app-data");
        let target_files_root = target.path().join("files-library");
        std::fs::create_dir_all(&target_app_data).unwrap();

        let target_db = target_app_data.join("lattice.db");
        let target_pool = seeded_pool(&target_db, 0).await;
        let target_key_store = isolated_key_store(&target_app_data);
        let target_settings: Arc<dyn SettingsRepositoryPort> =
            Arc::new(MockSettingsRepository::new());
        let restorer = ArchiveRestorer::new(
            target_pool.clone(),
            target_db.clone(),
            target_app_data.clone(),
            target_settings,
            target_key_store.clone(),
            Some(target_files_root.clone()),
        );

        // Without a secret there is nothing to unwrap the key with.
        assert!(matches!(
            restorer.restore(&run.path, None).await.unwrap(),
            RestoreOutcome::NeedsSecret
        ));

        // A wrong secret is distinguishable from a corrupt file.
        let wrong = restorer
            .restore(&run.path, Some("not the passphrase"))
            .await;
        assert!(matches!(wrong, Err(ArchiveError::WrongSecret)));

        // The recovery code alone gets everything back.
        let phrase = setup.recovery_words.join(" ");
        let outcome = restorer.restore(&run.path, Some(&phrase)).await.unwrap();
        let RestoreOutcome::Restored {
            restart_required,
            reembed_required,
            files_restored,
            ..
        } = outcome
        else {
            panic!("expected a completed restore, got {outcome:?}");
        };
        assert!(restart_required);
        assert!(reembed_required);
        assert_eq!(
            files_restored, 1,
            "only the referenced blob should have been in the archive"
        );

        assert_eq!(document_count(&target_db).await, 3, "rows should be back");
        assert_eq!(
            std::fs::read(target_files_root.join(REFERENCED_BLOB).join("paper.pdf")).unwrap(),
            b"pdf bytes"
        );
        assert!(
            !target_files_root.join(ORPHAN_BLOB).exists(),
            "an orphan blob was packed and restored"
        );
        assert!(
            !target_files_root.join("stray.txt").exists(),
            "a stray file was packed and restored"
        );
        // The source library keeps everything: a backup is not a sweep.
        assert!(files_root.join(ORPHAN_BLOB).join("ghost.pdf").exists());
        assert!(
            target_app_data
                .join(super::super::restore::REEMBED_MARKER)
                .exists(),
            "the corpus must be flagged for re-embedding"
        );
        assert!(
            target_key_store.load().unwrap().is_some(),
            "the unwrapped key should be adopted so backups can resume"
        );
        let adopted = ArchiveConfig::load(&target_app_data)
            .await
            .unwrap()
            .expect("a config should exist after a secret-based restore");
        assert!(adopted.is_configured());
        assert!(
            adopted.destination.is_none(),
            "the source machine's folder does not exist here"
        );

        pool.close().await;
    }
}
