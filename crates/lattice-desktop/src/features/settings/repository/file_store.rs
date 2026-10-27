//! The settings file on disk: atomic writes, corrupt-file quarantine and migration.
use super::{
    resolve_out_of_range_values, SettingsFile, SettingsRepository, SETTINGS_FILE_NAME,
    SETTINGS_VERSION, TEMP_SUFFIX,
};
use lattice_core::shared::error::{AppError, Result};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

impl SettingsRepository {
    /// Read settings file from disk.
    ///
    /// If the file is corrupted or has invalid format, returns default settings
    /// and overwrites the corrupted file.
    pub(super) async fn read_settings_file(&self) -> Result<SettingsFile> {
        let mut settings_file = match fs::read_to_string(&self.settings_path).await {
            Ok(content) => {
                match serde_json::from_str::<SettingsFile>(&content) {
                    Ok(settings_file) => {
                        if settings_file.version < SETTINGS_VERSION {
                            self.migrate_settings(settings_file).await?
                        } else {
                            settings_file
                        }
                    }
                    Err(e) => {
                        // Preserve the unparseable file before falling back —
                        // it is the only copy of the user's configuration.
                        self.quarantine_corrupt_settings(&e.to_string()).await;
                        let defaults = SettingsFile::default();
                        self.write_settings_file(&defaults).await?;
                        defaults
                    }
                }
            }
            Err(e) => {
                // File doesn't exist or can't be read - create defaults
                tracing::info!("Creating default settings file: {}", e);
                let defaults = SettingsFile::default();
                self.write_settings_file(&defaults).await?;
                defaults
            }
        };

        resolve_out_of_range_values(&mut settings_file.settings);

        Ok(settings_file)
    }

    /// Write settings file to disk using atomic write.
    ///
    /// Writes to a temporary file first, then renames to ensure atomicity.
    /// This prevents corruption if the application crashes during write.
    ///
    /// The temp filename is unique per write. A single shared `settings.tmp`
    /// is not safe: `File::create` truncates, so a second writer can truncate
    /// and rewrite the temp file while the first sits between `sync_all` and
    /// `rename`, renaming half-written JSON into place. That torn file then
    /// fails to parse on the next read.
    pub(super) async fn write_settings_file(&self, settings_file: &SettingsFile) -> Result<()> {
        // Serialize to JSON with pretty printing
        let json = serde_json::to_string_pretty(settings_file)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize settings: {}", e)))?;

        let temp_path = self.unique_temp_path();

        // Scoped so the handle is closed before the rename.
        {
            let mut file = fs::File::create(&temp_path).await.map_err(|e| {
                AppError::Storage(format!("Failed to create temp settings file: {}", e))
            })?;

            file.write_all(json.as_bytes()).await.map_err(|e| {
                AppError::Storage(format!("Failed to write temp settings file: {}", e))
            })?;

            // Ensure all data is flushed to disk
            file.sync_all().await.map_err(|e| {
                AppError::Storage(format!("Failed to sync temp settings file: {}", e))
            })?;
        }

        // Atomic rename - this is the critical operation
        if let Err(e) = fs::rename(&temp_path, &self.settings_path).await {
            // Don't leave the temp file behind on a failed rename.
            let _ = fs::remove_file(&temp_path).await;
            return Err(AppError::Storage(format!(
                "Failed to rename settings file: {}",
                e
            )));
        }

        Ok(())
    }

    /// A temp path unique to this write, alongside the real settings file so
    /// the rename stays within one filesystem.
    fn unique_temp_path(&self) -> PathBuf {
        let file_name = self
            .settings_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| SETTINGS_FILE_NAME.to_string());
        let unique = format!(".{}.{}{}", file_name, uuid::Uuid::new_v4(), TEMP_SUFFIX);
        match self.settings_path.parent() {
            Some(dir) => dir.join(unique),
            None => PathBuf::from(unique),
        }
    }

    /// Move an unparseable settings file aside instead of destroying it.
    ///
    /// Overwriting with defaults is unrecoverable: vault path, indexed
    /// folders, model roles and privacy flags are all gone with no copy. A
    /// parse failure is not proof the contents are worthless — a hand-edit
    /// typo, a torn write, or a field from a newer version all land here.
    async fn quarantine_corrupt_settings(&self, reason: &str) {
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let quarantine = self
            .settings_path
            .with_extension(format!("json.corrupt-{}", stamp));

        match fs::rename(&self.settings_path, &quarantine).await {
            Ok(()) => tracing::error!(
                original = %self.settings_path.display(),
                preserved_at = %quarantine.display(),
                reason,
                "Settings file could not be parsed; preserved a copy and continuing with defaults. \
                 Recover values from the preserved file."
            ),
            Err(e) => tracing::error!(
                original = %self.settings_path.display(),
                error = %e,
                reason,
                "Settings file could not be parsed AND could not be preserved; \
                 continuing with defaults"
            ),
        }
    }

    /// Migrate settings from older version to current version.
    async fn migrate_settings(&self, mut settings_file: SettingsFile) -> Result<SettingsFile> {
        tracing::info!(
            "Migrating settings from version {} to {}",
            settings_file.version,
            SETTINGS_VERSION
        );

        if settings_file.version < 2 {
            release_untouched_vector_compression(&mut settings_file.settings);
            release_untouched_answer_sampling(&mut settings_file.settings);
        }
        settings_file.version = SETTINGS_VERSION;

        self.write_settings_file(&settings_file).await?;

        Ok(settings_file)
    }
}

/// Version 1 stored vectors at 256 dims and i8 by default; version 2 stores
/// them at full precision. A document still on the old default never chose
/// compression, so it takes the new one (the index rebuilds on the next
/// start); any other setting is kept.
fn release_untouched_vector_compression(
    settings: &mut crate::features::settings::dto::SettingsDto,
) {
    use crate::features::settings::dto::{VectorIndexCompressionModeDto, VectorQuantizationDto};

    let stored = &mut settings.search.vector_index_compression;
    if stored.mode == VectorIndexCompressionModeDto::Truncated
        && stored.dims == 256
        && stored.quantization == VectorQuantizationDto::I8
    {
        stored.mode = VectorIndexCompressionModeDto::None;
    }
}

/// Version 1 sampled answers at top-p 0.9, top-k 40 and repeat penalty 1.1;
/// version 2 uses the Qwen 3.5 card's non-thinking profile (0.8, 20, 1.0,
/// with a presence penalty of 1.5 that older documents fill in on read).
/// Temperature was 0.7 in both. A value still exactly on the old default was
/// never tuned, so it moves; any other value was someone's choice and stays.
fn release_untouched_answer_sampling(settings: &mut crate::features::settings::dto::SettingsDto) {
    const OLD_TOP_P: f32 = 0.9;
    const OLD_TOP_K: i32 = 40;
    const OLD_REPEAT_PENALTY: f32 = 1.1;

    let defaults = crate::features::settings::dto::LLMSettingsDto::default();
    let stored = &mut settings.llm;
    if stored.top_p == OLD_TOP_P {
        stored.top_p = defaults.top_p;
    }
    if stored.top_k == OLD_TOP_K {
        stored.top_k = defaults.top_k;
    }
    if stored.repeat_penalty == OLD_REPEAT_PENALTY {
        stored.repeat_penalty = defaults.repeat_penalty;
    }
}
