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

use crate::features::credentials::keys::CredentialKey;
use crate::features::settings::dto::{
    LLMProvider, SettingsCategory, SettingsDto, ValidationResult,
    DEFAULT_LLM_STALL_TIMEOUT_SECONDS, MAX_LLM_STALL_TIMEOUT_SECONDS,
    MIN_LLM_STALL_TIMEOUT_SECONDS,
};
use async_trait::async_trait;
use lattice_core::application::ports::credentials_port::CredentialsPort;
use lattice_core::application::ports::settings_port::SettingsImportOutcome;
use lattice_core::application::ports::{merge_json_update, SettingsRepositoryPort};
use lattice_core::shared::error::{AppError, Result};
use serde_json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
/// 3: the cross-encoder judges 64 candidates by default, not 48.
const SETTINGS_VERSION: u32 = 3;

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

/// Brings a stored document inside the ranges the validator enforces.
/// `llm.timeout_seconds` is a stall timeout, so values saved under its former
/// total-timeout meaning fall back to the default rather than failing an
/// unrelated save or disabling stall detection.
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
    /// Where auth header values are kept. Without one, a value read from the
    /// file stays in it.
    credentials: Option<Arc<dyn CredentialsPort>>,
}

impl SettingsRepository {
    /// Create a new settings repository.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Cannot create app data directory
    /// - Cannot read or write settings file
    /// - Settings file is corrupted
    pub async fn new(app_data_dir: PathBuf) -> Result<Self> {
        Self::open(app_data_dir, None).await
    }

    /// A repository that keeps auth header values in `credentials`, moving any
    /// a settings file written before that still holds. The move is retried
    /// at every start until the keyring accepts the value; the file keeps it
    /// until then.
    pub async fn with_credentials(
        app_data_dir: PathBuf,
        credentials: Arc<dyn CredentialsPort>,
    ) -> Result<Self> {
        let repository = Self::open(app_data_dir, Some(credentials)).await?;
        if let Err(error) = repository.migrate_legacy_secrets().await {
            tracing::warn!(%error, "Could not move auth header values to the keyring; will retry at the next start");
        }
        Ok(repository)
    }

    async fn open(
        app_data_dir: PathBuf,
        credentials: Option<Arc<dyn CredentialsPort>>,
    ) -> Result<Self> {
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
            credentials,
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
        let mut settings = settings.clone();
        self.move_secrets_to_keyring(&mut settings).await;
        let validation = self.validate_settings(&settings);
        if validation.has_errors() {
            return Err(AppError::ValidationFailed(format!(
                "Settings validation failed: {:?}",
                validation.errors
            )));
        }

        let settings_file = SettingsFile {
            version: SETTINGS_VERSION,
            settings,
        };

        self.write_settings_file(&settings_file).await
    }

    /// Store any auth header value `settings` carries in the keyring and drop
    /// it from `settings`. A value the keyring refuses stays, so the file
    /// still holds it and nothing is lost.
    async fn move_secrets_to_keyring(&self, settings: &mut SettingsDto) {
        let Some(credentials) = &self.credentials else {
            return;
        };
        let llm = &mut settings.llm;
        llm.ollama_auth_header_value_set = false;
        llm.llama_cpp.auth_header_value_set = false;
        for (key, value) in [
            (
                CredentialKey::OllamaAuthHeader,
                &mut llm.ollama_auth_header_value,
            ),
            (
                CredentialKey::LlamaCppAuthHeader,
                &mut llm.llama_cpp.auth_header_value,
            ),
        ] {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                value.clear();
                continue;
            }
            match credentials.set_api_key(key.as_str(), trimmed).await {
                Ok(()) => value.clear(),
                Err(error) => tracing::warn!(
                    credential = key.as_str(),
                    %error,
                    "Could not move an auth header value to the keyring; keeping it in the settings file until it can"
                ),
            }
        }
    }

    /// Move the auth header values a settings file written before they lived
    /// in the keyring still holds.
    async fn migrate_legacy_secrets(&self) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let mut settings_file = self.read_settings_file().await?;
        let held = |settings: &SettingsDto| {
            [
                &settings.llm.ollama_auth_header_value,
                &settings.llm.llama_cpp.auth_header_value,
            ]
            .iter()
            .filter(|value| !value.trim().is_empty())
            .count()
        };
        let before = held(&settings_file.settings);
        if before == 0 {
            return Ok(());
        }
        self.move_secrets_to_keyring(&mut settings_file.settings)
            .await;
        if held(&settings_file.settings) == before {
            return Ok(());
        }
        self.write_settings_file(&settings_file).await
    }

    /// Refuse a header name saved with no value to go with it, in the update
    /// itself or already in the keyring. Names `updates` leaves alone are not
    /// checked, so a keyring that lost a value does not stop unrelated
    /// settings from saving.
    async fn require_header_values(
        &self,
        llm: &crate::features::settings::dto::LLMSettingsDto,
        updates: &HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let Some(credentials) = &self.credentials else {
            return Ok(());
        };
        let ollama_edited = updates.contains_key("ollamaAuthHeaderName");
        let llama_edited = updates
            .get("llamaCpp")
            .is_some_and(|connection| connection.get("authHeaderName").is_some());
        if ollama_edited
            && !llm.ollama_auth_header_name.trim().is_empty()
            && llm.ollama_auth_header_value.trim().is_empty()
            && !credentials
                .has_api_key(CredentialKey::OllamaAuthHeader.as_str())
                .await?
        {
            return Err(AppError::ValidationFailed(
                "ollama_auth_header_name and ollama_auth_header_value must both be set".to_string(),
            ));
        }
        if llama_edited
            && !llm.llama_cpp.auth_header_name.trim().is_empty()
            && llm.llama_cpp.auth_header_value.trim().is_empty()
            && !credentials
                .has_api_key(CredentialKey::LlamaCppAuthHeader.as_str())
                .await?
        {
            return Err(AppError::ValidationFailed(
                "Set both authentication header fields, or clear both".to_string(),
            ));
        }
        Ok(())
    }

    /// Delete the stored auth header values.
    async fn forget_header_secrets(&self) {
        let Some(credentials) = &self.credentials else {
            return;
        };
        for key in [
            CredentialKey::OllamaAuthHeader,
            CredentialKey::LlamaCppAuthHeader,
        ] {
            if let Err(error) = credentials.delete_api_key(key.as_str()).await {
                tracing::warn!(credential = key.as_str(), %error, "Could not delete a stored auth header value");
            }
        }
    }

    /// Whether a value is held for `key`, in the keyring or still in settings.
    async fn header_value_held(&self, key: CredentialKey, in_settings: &str) -> bool {
        if !in_settings.trim().is_empty() {
            return true;
        }
        match &self.credentials {
            Some(credentials) => credentials.has_api_key(key.as_str()).await.unwrap_or(false),
            None => false,
        }
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
        let ollama_held = self
            .header_value_held(
                CredentialKey::OllamaAuthHeader,
                &existing.llm.ollama_auth_header_value,
            )
            .await;
        let llama_held = self
            .header_value_held(
                CredentialKey::LlamaCppAuthHeader,
                &existing.llm.llama_cpp.auth_header_value,
            )
            .await;
        drop_unbacked_header_names(&mut imported_settings, &existing, ollama_held, llama_held);

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
            existing.sourcing = imported_settings.sourcing;

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
            SettingsCategory::Sourcing => serde_json::to_value(&settings.sourcing)?,
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
                    SettingsCategory::Sourcing => serde_json::to_value(&settings.sourcing)?,
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
                        let held_ollama = settings.llm.ollama_auth_header_value.clone();
                        let held_llama = settings.llm.llama_cpp.auth_header_value.clone();
                        settings.llm = serde_json::from_value(merged)?;
                        if settings.llm.ollama_auth_header_value.is_empty() {
                            settings.llm.ollama_auth_header_value = held_ollama;
                        }
                        if settings.llm.llama_cpp.auth_header_value.is_empty() {
                            settings.llm.llama_cpp.auth_header_value = held_llama;
                        }
                        self.require_header_values(&settings.llm, &updates).await?;
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
                    SettingsCategory::Sourcing => {
                        settings.sourcing = serde_json::from_value(merged)?;
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

        if matches!(category, None | Some(SettingsCategory::Llm)) {
            self.forget_header_secrets().await;
        }

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
                SettingsCategory::Sourcing => settings.sourcing = Default::default(),
            },
            None => {
                settings = SettingsDto::default();
            }
        }

        self.persist(&settings).await?;

        Ok(settings)
    }

    async fn export(&self, path: &str) -> Result<()> {
        // The remote servers' auth header values are bearer tokens in the
        // keyring and are never serialized, so an export carries the header
        // names only; an import keeps this machine's values (see
        // `drop_unbacked_header_names`).
        let settings = self.get_all().await?;

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

/// Keep an imported header name only when this machine holds the value for
/// that very header, since an export never carries values. A header whose
/// value is not here (another machine, or a different header) loses its name
/// too: a name without a value is useless to a request, and the user
/// re-enters both.
fn drop_unbacked_header_names(
    imported: &mut SettingsDto,
    existing: &SettingsDto,
    ollama_held: bool,
    llama_held: bool,
) {
    fn keep(name: &mut String, value: &str, existing_name: &str, held: bool) {
        if !value.trim().is_empty() || name.trim().is_empty() {
            return;
        }
        if !(held && name.trim() == existing_name.trim()) {
            name.clear();
        }
    }
    let llm = &mut imported.llm;
    keep(
        &mut llm.ollama_auth_header_name,
        &llm.ollama_auth_header_value,
        &existing.llm.ollama_auth_header_name,
        ollama_held,
    );
    keep(
        &mut llm.llama_cpp.auth_header_name,
        &llm.llama_cpp.auth_header_value,
        &existing.llm.llama_cpp.auth_header_name,
        llama_held,
    );
}

#[cfg(test)]
mod tests;
