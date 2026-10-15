//! # Settings Repository Implementation
//!
//! File-based JSON storage for application settings with atomic writes.
//!
//! This implementation provides persistent storage for application settings using
//! JSON files. It ensures data integrity through atomic write operations and
//! includes validation, default settings, and import/export.
//!
//! # Features
//!
//! - **Atomic Writes**: Write to temp file, then rename for atomicity
//! - **Default Settings**: Factory for sensible defaults
//! - **Validation**: Integrated validation for all read/write operations
//! - **Import/Export**: Support for settings backup and transfer
//! - **Thread Safety**: Read-modify-write operations are serialized by a mutex

use crate::features::settings::dto::{
    LLMProvider, SettingsCategory, SettingsDto, ValidationResult,
    DEFAULT_LLM_STALL_TIMEOUT_SECONDS, MAX_LLM_STALL_TIMEOUT_SECONDS,
    MIN_LLM_STALL_TIMEOUT_SECONDS,
};
use async_trait::async_trait;
use lattice_core::application::ports::settings_port::SettingsImportOutcome;
use lattice_core::application::ports::{merge_json_update, SettingsRepositoryPort};
use lattice_core::shared::error::{AppError, Result};
use serde_json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

mod file_store;
mod validation;

/// File name for settings storage.
const SETTINGS_FILE_NAME: &str = "settings.json";

/// Temporary file suffix for atomic writes.
const TEMP_SUFFIX: &str = ".tmp";

/// Settings file version for migration support.
///
/// 2: full-precision vectors became the default (see `file_store::migrate_settings`).
const SETTINGS_VERSION: u32 = 2;

/// Settings file structure with version.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SettingsFile {
    version: u32,
    settings: SettingsDto,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            settings: SettingsDto::default(),
        }
    }
}

/// Bring a stored document inside the ranges the validator enforces.
///
/// `llm.timeout_seconds` used to mean "total request timeout" and now means
/// "stall timeout", so values written under the old meaning — 120 for a cloud
/// provider, 300 to escape the old five-minute cap — land outside the new
/// range. Resolving them to the default keeps them away from the validator,
/// which would otherwise reject a save of some unrelated field with an error
/// about a number the user never typed, and stops a stale 300 from disabling
/// stall detection outright.
fn resolve_out_of_range_values(settings: &mut SettingsDto) {
    if !(MIN_LLM_STALL_TIMEOUT_SECONDS..=MAX_LLM_STALL_TIMEOUT_SECONDS)
        .contains(&settings.llm.timeout_seconds)
    {
        tracing::info!(
            stored = settings.llm.timeout_seconds,
            resolved = DEFAULT_LLM_STALL_TIMEOUT_SECONDS,
            "Stored LLM stall timeout is outside the supported range; using the default"
        );
        settings.llm.timeout_seconds = DEFAULT_LLM_STALL_TIMEOUT_SECONDS;
    }
}

/// File-based settings repository implementation.
///
/// Stores settings as JSON in the application data directory.
/// Provides atomic writes to prevent corruption.
///
/// # Thread Safety
///
/// Multiple concurrent reads are safe. `update`, `reset`, and `import` hold
/// `write_lock` across their whole read-merge-write cycle, so concurrent
/// updates to different categories cannot lose each other. Each write goes
/// to a uniquely-named temp file before an atomic rename, so a write that
/// is interrupted never leaves partial JSON in place.
///
/// `save_all` also takes the lock, but intentionally replaces the whole document.
/// Callers changing individual fields should use `update` to preserve other writes.
///
/// # Example
///
/// ```rust,ignore
/// use crate::features::settings::repository::SettingsRepository;
/// use tauri::api::path::app_data_dir;
///
/// let app_data_dir = app_data_dir(&config).expect("Failed to get app data dir");
/// let repository = SettingsRepository::new(app_data_dir).await?;
///
/// let settings = repository.get_all().await?;
/// println!("Chunk size: {}", settings.indexing.chunk_size);
/// ```
pub struct SettingsRepository {
    /// Path to settings file
    settings_path: PathBuf,
    /// Serializes settings mutations, including import and secret restoration.
    ///
    /// Read-modify-write operations hold it from `get_all()` through `persist()`.
    /// Without a lock, two concurrent updates to different categories both
    /// read the same base state and the second write silently reverts the
    /// first — the UI reports success for both. The repository is shared
    /// behind an `Arc`, so a single mutex here is enough to make the whole
    /// cycle atomic.
    write_lock: tokio::sync::Mutex<()>,
}

impl SettingsRepository {
    /// Create a new settings repository.
    ///
    /// # Arguments
    ///
    /// * `app_data_dir` - Application data directory path
    ///
    /// # Returns
    ///
    /// Initialized repository with default settings if file doesn't exist
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Cannot create app data directory
    /// - Cannot read or write settings file
    /// - Settings file is corrupted
    pub async fn new(app_data_dir: PathBuf) -> Result<Self> {
        // repository-barrier-allow: this repository's resource is the settings file and its containing directory.
        // Ensure app data directory exists
        if !app_data_dir.exists() {
            fs::create_dir_all(&app_data_dir).await.map_err(|e| {
                AppError::Storage(format!("Failed to create app data directory: {}", e))
            })?;
        }

        let settings_path = app_data_dir.join(SETTINGS_FILE_NAME);

        let repository = Self {
            settings_path,
            write_lock: tokio::sync::Mutex::new(()),
        };

        // repository-barrier-allow: the settings repository owns existence of its file resource.
        // Initialize with defaults if file doesn't exist
        if !repository.settings_path.exists() {
            repository
                .write_settings_file(&SettingsFile::default())
                .await?;
        }

        Ok(repository)
    }

    /// Validate and atomically replace the file. The caller holds `write_lock`.
    async fn persist(&self, settings: &SettingsDto) -> Result<()> {
        let validation = self.validate_settings(settings);
        if validation.has_errors() {
            return Err(AppError::ValidationFailed(format!(
                "Settings validation failed: {:?}",
                validation.errors
            )));
        }

        let settings_file = SettingsFile {
            version: SETTINGS_VERSION,
            settings: settings.clone(),
        };

        self.write_settings_file(&settings_file).await
    }

    async fn import_settings(
        &self,
        mut imported_settings: SettingsDto,
        merge: bool,
    ) -> Result<SettingsImportOutcome> {
        // An export taken before the stall-timeout rework is otherwise rejected
        // wholesale over one repurposed number.
        resolve_out_of_range_values(&mut imported_settings);

        let _guard = self.write_lock.lock().await;
        let existing = self.get_all().await?;
        let previous_vault_enabled = existing.vault.enabled;
        restore_redacted_secrets(&mut imported_settings, &existing);

        let validation = self.validate_settings(&imported_settings);
        if validation.has_errors() {
            return Err(AppError::ValidationFailed(format!(
                "Imported settings validation failed: {:?}",
                validation.errors
            )));
        }

        let final_settings = if merge {
            let mut existing = existing;

            existing.indexing = imported_settings.indexing;
            existing.search = imported_settings.search;
            existing.llm = imported_settings.llm;
            existing.ui = imported_settings.ui;
            existing.sync = imported_settings.sync;
            existing.backup = imported_settings.backup;
            existing.privacy = imported_settings.privacy;

            existing
        } else {
            imported_settings
        };

        self.persist(&final_settings).await?;

        Ok(SettingsImportOutcome {
            vault_just_enabled: !previous_vault_enabled && final_settings.vault.enabled,
            settings: final_settings,
        })
    }

    /// Merge updates into a category.
    fn merge_category_updates(
        &self,
        category_value: serde_json::Value,
        updates: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let mut category_map = category_value
            .as_object()
            .ok_or_else(|| AppError::InvalidInput("Category is not an object".to_string()))?
            .clone();

        for (key, value) in updates {
            match category_map.get_mut(key) {
                Some(existing) => merge_json_update(existing, value.clone()),
                None => {
                    category_map.insert(key.clone(), value.clone());
                }
            }
        }

        Ok(serde_json::Value::Object(category_map))
    }
}

#[async_trait]
impl SettingsRepositoryPort for SettingsRepository {
    async fn get_all(&self) -> Result<SettingsDto> {
        let settings_file = self.read_settings_file().await?;
        Ok(settings_file.settings)
    }

    async fn get_category(&self, category: SettingsCategory) -> Result<serde_json::Value> {
        let settings = self.get_all().await?;

        let value = match category {
            SettingsCategory::Indexing => serde_json::to_value(&settings.indexing)?,
            SettingsCategory::Search => serde_json::to_value(&settings.search)?,
            SettingsCategory::Llm => serde_json::to_value(&settings.llm)?,
            SettingsCategory::Ui => serde_json::to_value(&settings.ui)?,
            SettingsCategory::Sync => serde_json::to_value(&settings.sync)?,
            SettingsCategory::Backup => serde_json::to_value(&settings.backup)?,
            SettingsCategory::Privacy => serde_json::to_value(&settings.privacy)?,
            SettingsCategory::Vault => serde_json::to_value(&settings.vault)?,
            SettingsCategory::Onboarding => serde_json::to_value(&settings.onboarding)?,
        };

        Ok(value)
    }

    async fn save_all(&self, settings: &SettingsDto) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        self.persist(settings).await
    }

    async fn update(
        &self,
        category: Option<SettingsCategory>,
        updates: HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto> {
        // Held for the whole read-merge-write cycle. Dropping it earlier
        // reintroduces the lost-update race this lock exists to prevent.
        let _guard = self.write_lock.lock().await;

        let mut settings = self.get_all().await?;

        match category {
            Some(cat) => {
                let category_value = match cat {
                    SettingsCategory::Indexing => serde_json::to_value(&settings.indexing)?,
                    SettingsCategory::Search => serde_json::to_value(&settings.search)?,
                    SettingsCategory::Llm => serde_json::to_value(&settings.llm)?,
                    SettingsCategory::Ui => serde_json::to_value(&settings.ui)?,
                    SettingsCategory::Sync => serde_json::to_value(&settings.sync)?,
                    SettingsCategory::Backup => serde_json::to_value(&settings.backup)?,
                    SettingsCategory::Privacy => serde_json::to_value(&settings.privacy)?,
                    SettingsCategory::Vault => serde_json::to_value(&settings.vault)?,
                    SettingsCategory::Onboarding => serde_json::to_value(&settings.onboarding)?,
                };

                let merged = self.merge_category_updates(category_value, &updates)?;

                match cat {
                    SettingsCategory::Indexing => {
                        settings.indexing = serde_json::from_value(merged)?;
                    }
                    SettingsCategory::Search => {
                        settings.search = serde_json::from_value(merged)?;
                    }
                    SettingsCategory::Llm => {
                        let previously_selected = settings.llm.provider == LLMProvider::Ollama;
                        settings.llm = serde_json::from_value(merged)?;
                        // Keep an explicitly configured connection discoverable after
                        // switching providers. The shared `model` field also stores
                        // cloud model names, so it cannot establish this on its own.
                        if !updates.contains_key("ollamaConfigured")
                            && (previously_selected
                                || settings.llm.provider == LLMProvider::Ollama
                                || [
                                    "ollamaUrl",
                                    "ollamaUtilityModel",
                                    "ollamaAuthHeaderName",
                                    "ollamaAuthHeaderValue",
                                ]
                                .iter()
                                .any(|key| updates.contains_key(*key)))
                        {
                            settings.llm.ollama_configured = true;
                        }
                    }
                    SettingsCategory::Ui => {
                        settings.ui = serde_json::from_value(merged)?;
                    }
                    SettingsCategory::Sync => {
                        settings.sync = serde_json::from_value(merged)?;
                    }
                    SettingsCategory::Backup => {
                        settings.backup = serde_json::from_value(merged)?;
                    }
                    SettingsCategory::Privacy => {
                        settings.privacy = serde_json::from_value(merged)?;
                    }
                    SettingsCategory::Vault => {
                        settings.vault = serde_json::from_value(merged)?;
                    }
                    SettingsCategory::Onboarding => {
                        settings.onboarding = serde_json::from_value(merged)?;
                    }
                }
            }
            None => {
                let mut settings_value = serde_json::to_value(&settings)?;
                let settings_map = settings_value.as_object_mut().ok_or_else(|| {
                    AppError::InvalidInput("Settings is not an object".to_string())
                })?;

                for (key, value) in updates {
                    match settings_map.get_mut(&key) {
                        Some(existing) => merge_json_update(existing, value),
                        None => {
                            settings_map.insert(key, value);
                        }
                    }
                }

                settings = serde_json::from_value(serde_json::Value::Object(settings_map.clone()))?;
            }
        }

        self.persist(&settings).await?;

        Ok(settings)
    }

    async fn reset(&self, category: Option<SettingsCategory>) -> Result<SettingsDto> {
        // Same read-modify-write cycle as `update`, same lock.
        let _guard = self.write_lock.lock().await;

        let mut settings = self.get_all().await?;

        match category {
            Some(cat) => match cat {
                SettingsCategory::Indexing => settings.indexing = Default::default(),
                SettingsCategory::Search => settings.search = Default::default(),
                SettingsCategory::Llm => settings.llm = Default::default(),
                SettingsCategory::Ui => settings.ui = Default::default(),
                SettingsCategory::Sync => settings.sync = Default::default(),
                SettingsCategory::Backup => settings.backup = Default::default(),
                SettingsCategory::Privacy => settings.privacy = Default::default(),
                SettingsCategory::Vault => settings.vault = Default::default(),
                SettingsCategory::Onboarding => settings.onboarding = Default::default(),
            },
            None => {
                settings = SettingsDto::default();
            }
        }

        self.persist(&settings).await?;

        Ok(settings)
    }

    async fn export(&self, path: &str) -> Result<()> {
        let mut settings = self.get_all().await?;
        // The remote servers' auth headers are bearer tokens. An export is a
        // file the user moves around and shares; it carries the header names
        // and the import puts back this machine's tokens (see
        // `restore_redacted_secrets`).
        settings.llm.ollama_auth_header_value.clear();
        settings.llm.llama_cpp.auth_header_value.clear();

        let json = serde_json::to_string_pretty(&settings)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize settings: {}", e)))?;

        fs::write(path, json)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to write export file: {}", e)))?;

        Ok(())
    }

    async fn import(&self, path: &str, merge: bool) -> Result<SettingsImportOutcome> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to read import file: {}", e)))?;

        let imported_settings: SettingsDto = serde_json::from_str(&content).map_err(|e| {
            AppError::Deserialization(format!("Failed to deserialize settings: {}", e))
        })?;

        self.import_settings(imported_settings, merge).await
    }

    fn validate(&self, settings: &SettingsDto) -> ValidationResult {
        self.validate_settings(settings)
    }

    fn validate_folder_path(&self, path: &str) -> bool {
        let path = Path::new(path);
        // repository-barrier-allow: settings validation checks a user-selected folder resource.
        path.exists() && path.is_dir()
    }
}

/// Put this machine's auth tokens back into an imported export, which never
/// carries them. A header whose token is not here (another machine, or a
/// different header) loses its name too: a name without a token is refused by
/// validation and useless to a request, and the user re-enters both.
fn restore_redacted_secrets(imported: &mut SettingsDto, existing: &SettingsDto) {
    fn restore(name: &mut String, value: &mut String, existing_name: &str, existing_value: &str) {
        if !value.trim().is_empty() || name.trim().is_empty() {
            return;
        }
        if name.trim() == existing_name.trim() && !existing_value.trim().is_empty() {
            *value = existing_value.to_string();
        } else {
            name.clear();
        }
    }
    let llm = &mut imported.llm;
    restore(
        &mut llm.ollama_auth_header_name,
        &mut llm.ollama_auth_header_value,
        &existing.llm.ollama_auth_header_name,
        &existing.llm.ollama_auth_header_value,
    );
    restore(
        &mut llm.llama_cpp.auth_header_name,
        &mut llm.llama_cpp.auth_header_value,
        &existing.llm.llama_cpp.auth_header_name,
        &existing.llm.llama_cpp.auth_header_value,
    );
}

#[cfg(test)]
mod tests;
