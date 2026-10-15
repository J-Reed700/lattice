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
mod tests;
