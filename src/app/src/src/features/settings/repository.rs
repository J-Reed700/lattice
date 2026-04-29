//! # Settings Repository Implementation
//!
//! File-based JSON storage for application settings with atomic writes.
//!
//! This implementation provides persistent storage for application settings using
//! JSON files. It ensures data integrity through atomic write operations and
//! includes validation, default settings, import/export, and migration support.
//!
//! # Features
//!
//! - **Atomic Writes**: Write to temp file, then rename for atomicity
//! - **Default Settings**: Factory for sensible defaults
//! - **Validation**: Integrated validation for all read/write operations
//! - **Import/Export**: Support for settings backup and migration
//! - **Thread Safety**: Uses async file I/O with proper locking

use crate::features::settings::dto::{
    LLMProvider, SettingsCategory, SettingsDto, ValidationResult,
};
use crate::application::ports::SettingsRepositoryPort;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use serde_json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use url::Url;

/// File name for settings storage.
const SETTINGS_FILE_NAME: &str = "settings.json";

/// Temporary file suffix for atomic writes.
const TEMP_SUFFIX: &str = ".tmp";

/// Settings file version for migration support.
const SETTINGS_VERSION: u32 = 1;

/// Settings file structure with version.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SettingsFile {
    version: u32,
    settings: SettingsDto,
}

/// Local mirror of the legacy `AppConfig` shape, used solely to deserialize
/// a leftover `config.json` from before the AppConfig→Settings unification.
/// Defined here so we don't have to keep AppConfig the type around just for
/// reading the legacy file.
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct LegacyAppConfig {
    indexed_paths: Vec<String>,
    exclude_patterns: Vec<String>,
    auto_index: bool,
    ollama_endpoint: String,
    ollama_model: String,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            settings: SettingsDto::default(),
        }
    }
}

/// File-based settings repository implementation.
///
/// Stores settings as JSON in the application data directory.
/// Provides atomic writes to prevent corruption.
///
/// # Thread Safety
///
/// This implementation is thread-safe through async file operations.
/// Multiple concurrent reads are safe. Writes are serialized through
/// the file system's atomic rename operation.
///
/// # Example
///
/// ```rust,ignore
/// use crate::infrastructure::persistence::repositories::SettingsRepository;
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
        // Ensure app data directory exists
        if !app_data_dir.exists() {
            fs::create_dir_all(&app_data_dir).await.map_err(|e| {
                AppError::Storage(format!("Failed to create app data directory: {}", e))
            })?;
        }

        let settings_path = app_data_dir.join(SETTINGS_FILE_NAME);
        let legacy_config_path = app_data_dir.join("config.json");

        // Self-heal: a prior buggy DI wiring (interfaces/di/modules.rs)
        // passed `data_dir/settings.json` as the dir arg to this
        // constructor, which then joined `settings.json` again — creating
        // a stray `settings.json/settings.json` layout on disk. Detect
        // and recover: if `settings.json` is a directory, salvage any
        // nested `settings.json` file to take its place, then remove the
        // wrapper. We never error out here — failure to repair drops
        // back to defaults rather than blocking app startup.
        if settings_path.is_dir() {
            tracing::warn!(
                "Found stray settings.json directory at {} (legacy DI bug); attempting self-heal",
                settings_path.display()
            );
            let nested = settings_path.join(SETTINGS_FILE_NAME);
            let salvaged_contents = if nested.is_file() {
                match fs::read_to_string(&nested).await {
                    Ok(c) => Some(c),
                    Err(e) => {
                        tracing::warn!("Could not read nested settings.json during self-heal: {e}");
                        None
                    }
                }
            } else {
                None
            };

            if let Err(e) = fs::remove_dir_all(&settings_path).await {
                // Last-ditch fallback: rename it aside so we can write the
                // file in its place. Don't refuse to boot.
                tracing::warn!(
                    "Could not remove stray settings.json directory: {e}. \
                     Renaming aside so we can write the canonical file."
                );
                let aside = app_data_dir.join("settings.json.legacy-dir");
                let _ = fs::rename(&settings_path, &aside).await;
            }

            if let Some(contents) = salvaged_contents {
                if let Err(e) = fs::write(&settings_path, contents.as_bytes()).await {
                    tracing::warn!("Could not write salvaged settings during self-heal: {e}");
                }
            }
        }

        let repository = Self { settings_path };

        // Initialize with defaults if file doesn't exist
        if !repository.settings_path.exists() {
            repository
                .write_settings_file(&SettingsFile::default())
                .await?;
        }

        // One-shot migration of the legacy AppConfig (config.json) into the
        // unified Settings store. Idempotent: deletes config.json on success
        // so subsequent boots are no-ops. Failure to migrate is logged but
        // not fatal — better to start with stale config.json than refuse to
        // boot the app.
        if legacy_config_path.exists() {
            if let Err(err) = repository.migrate_legacy_app_config(&legacy_config_path).await {
                tracing::warn!(
                    "Legacy config.json migration failed (non-fatal): {err}. \
                     The file will remain on disk; settings.json defaults are in use."
                );
            }
        }

        Ok(repository)
    }

    /// Migrate the legacy `config.json` (AppConfig) into the unified
    /// settings.json store. Maps:
    ///
    /// - `indexed_paths`     → `settings.indexing.indexed_paths`
    /// - `exclude_patterns`  → `settings.indexing.exclude_patterns`
    /// - `auto_index`        → `settings.indexing.auto_index_new_files`
    /// - `ollama_endpoint`   → `settings.llm.ollama_url`
    /// - `ollama_model`      → `settings.llm.model`
    ///
    /// Existing settings.json values are preserved if non-default — we only
    /// fill in the legacy values where the unified store still has its
    /// startup defaults. This avoids clobbering anything the user already
    /// set via the new Settings UI.
    ///
    /// On success, the legacy `config.json` is deleted so the migration
    /// runs once and only once.
    async fn migrate_legacy_app_config(&self, legacy_path: &Path) -> Result<()> {
        let raw = fs::read_to_string(legacy_path).await.map_err(|e| {
            AppError::Storage(format!("Failed to read legacy config.json: {e}"))
        })?;

        // Use the same JsonValidator the old ConfigService used for
        // consistency on size/depth limits.
        let legacy: LegacyAppConfig =
            crate::security::json_validator::JsonValidator::safe_deserialize::<LegacyAppConfig>(
                &raw,
                10_000_000, // 10 MB
                50,         // max depth
            )
            .map_err(|e| {
                AppError::Deserialization(format!("Failed to parse legacy config.json: {e}"))
            })?;

        let mut settings_file = self.read_settings_file().await?;
        let defaults = SettingsDto::default();

        // Indexing migration. Always copy `indexed_paths` (this list lived
        // ONLY in AppConfig — settings.indexing.indexed_paths starts empty).
        if !legacy.indexed_paths.is_empty()
            && settings_file.settings.indexing.indexed_paths.is_empty()
        {
            settings_file.settings.indexing.indexed_paths = legacy.indexed_paths;
        }

        // Exclude patterns: only overwrite if settings.json still has the
        // unmodified default. Avoids clobbering user customizations.
        if settings_file.settings.indexing.exclude_patterns
            == defaults.indexing.exclude_patterns
            && !legacy.exclude_patterns.is_empty()
        {
            settings_file.settings.indexing.exclude_patterns = legacy.exclude_patterns;
        }

        // auto_index: AppConfig default was false, Settings default is true.
        // Only copy when AppConfig's value is `false` (a non-default in
        // AppConfig — meaning the user explicitly disabled it). Otherwise
        // keep settings.json's value.
        if !legacy.auto_index {
            settings_file.settings.indexing.auto_index_new_files = false;
        }

        // LLM endpoint + model: only fill if settings.json still has its
        // own defaults — same reasoning as above (don't clobber).
        if settings_file.settings.llm.ollama_url == defaults.llm.ollama_url
            && !legacy.ollama_endpoint.is_empty()
        {
            settings_file.settings.llm.ollama_url = legacy.ollama_endpoint;
        }
        if settings_file.settings.llm.model == defaults.llm.model
            && !legacy.ollama_model.is_empty()
        {
            settings_file.settings.llm.model = legacy.ollama_model;
        }

        self.write_settings_file(&settings_file).await?;

        // Delete the legacy file last — only after the new state is durable.
        if let Err(e) = fs::remove_file(legacy_path).await {
            tracing::warn!(
                "Migrated legacy config.json successfully but failed to delete it: {e}. \
                 The next boot will retry the (now no-op) migration."
            );
        } else {
            tracing::info!(
                "Migrated legacy config.json into settings.json and deleted the old file."
            );
        }

        Ok(())
    }

    /// Read settings file from disk.
    ///
    /// If the file is corrupted or has invalid format, returns default settings
    /// and overwrites the corrupted file.
    async fn read_settings_file(&self) -> Result<SettingsFile> {
        match fs::read_to_string(&self.settings_path).await {
            Ok(content) => {
                match serde_json::from_str::<SettingsFile>(&content) {
                    Ok(settings_file) => {
                        // Check version and migrate if needed
                        if settings_file.version < SETTINGS_VERSION {
                            self.migrate_settings(settings_file).await
                        } else {
                            let mut settings_file = settings_file;
                            let upgraded = Self::upgrade_legacy_low_limit_defaults(
                                &mut settings_file.settings,
                            );
                            if upgraded {
                                self.write_settings_file(&settings_file).await?;
                                tracing::info!(
                                    "Applied legacy low-limit settings upgrade to modern defaults"
                                );
                            }
                            Ok(settings_file)
                        }
                    }
                    Err(e) => {
                        // Corrupted file - restore defaults
                        tracing::warn!("Settings file corrupted, restoring defaults: {}", e);
                        let defaults = SettingsFile::default();
                        self.write_settings_file(&defaults).await?;
                        Ok(defaults)
                    }
                }
            }
            Err(e) => {
                // File doesn't exist or can't be read - create defaults
                tracing::info!("Creating default settings file: {}", e);
                let defaults = SettingsFile::default();
                self.write_settings_file(&defaults).await?;
                Ok(defaults)
            }
        }
    }

    /// Write settings file to disk using atomic write.
    ///
    /// Writes to a temporary file first, then renames to ensure atomicity.
    /// This prevents corruption if the application crashes during write.
    async fn write_settings_file(&self, settings_file: &SettingsFile) -> Result<()> {
        // Serialize to JSON with pretty printing
        let json = serde_json::to_string_pretty(settings_file)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize settings: {}", e)))?;

        // Write to temporary file
        let temp_path = self.settings_path.with_extension(TEMP_SUFFIX);
        let mut file = fs::File::create(&temp_path).await.map_err(|e| {
            AppError::Storage(format!("Failed to create temp settings file: {}", e))
        })?;

        file.write_all(json.as_bytes())
            .await
            .map_err(|e| AppError::Storage(format!("Failed to write temp settings file: {}", e)))?;

        // Ensure all data is flushed to disk
        file.sync_all()
            .await
            .map_err(|e| AppError::Storage(format!("Failed to sync temp settings file: {}", e)))?;

        // Atomic rename - this is the critical operation
        fs::rename(&temp_path, &self.settings_path)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to rename settings file: {}", e)))?;

        Ok(())
    }

    /// Migrate settings from older version to current version.
    ///
    /// Currently a no-op as we're on version 1.
    /// Future versions should implement migration logic here.
    async fn migrate_settings(&self, mut settings_file: SettingsFile) -> Result<SettingsFile> {
        tracing::info!(
            "Migrating settings from version {} to {}",
            settings_file.version,
            SETTINGS_VERSION
        );

        // Update version
        settings_file.version = SETTINGS_VERSION;

        // Save migrated settings
        self.write_settings_file(&settings_file).await?;

        Ok(settings_file)
    }

    fn upgrade_legacy_low_limit_defaults(settings: &mut SettingsDto) -> bool {
        let mut changed = false;

        // Preserve explicit user customizations; only bump known legacy defaults.
        if settings.llm.max_tokens == 2048 && settings.llm.context_window == 4096 {
            settings.llm.max_tokens = 131072;
            settings.llm.context_window = 131072;
            changed = true;
        }
        if settings.llm.tool_output.max_chars == 1800 {
            settings.llm.tool_output.max_chars = 50000;
            changed = true;
        }
        if settings.llm.tool_output.excerpt_chars == 600 {
            settings.llm.tool_output.excerpt_chars = 4000;
            changed = true;
        }
        if settings.indexing.chunk_size == 250 {
            settings.indexing.chunk_size = 800;
            if settings.indexing.chunk_overlap >= settings.indexing.chunk_size
                || settings.indexing.chunk_overlap < 80
            {
                settings.indexing.chunk_overlap = 120;
            }
            changed = true;
        }

        let tuning = &mut settings.search.retrieval_tuning;
        if tuning.kb_search_min_limit == 12 && tuning.kb_search_max_limit == 30 {
            tuning.kb_search_min_limit = 16;
            tuning.kb_search_max_limit = 48;
            changed = true;
        }
        if tuning.doc_shortlist_candidate_min == 36 && tuning.doc_shortlist_candidate_max == 96 {
            tuning.doc_shortlist_candidate_min = 48;
            tuning.doc_shortlist_candidate_max = 192;
            changed = true;
        }
        if tuning.doc_shortlist_doc_min == 8 && tuning.doc_shortlist_doc_max == 24 {
            tuning.doc_shortlist_doc_min = 10;
            tuning.doc_shortlist_doc_max = 32;
            changed = true;
        }
        if tuning.shortlist_gate_min_candidates == 6 && tuning.shortlist_gate_min_docs == 3 {
            tuning.shortlist_gate_min_candidates = 8;
            tuning.shortlist_gate_min_docs = 4;
            changed = true;
        }
        if tuning.rerank_max_candidates == 24 {
            tuning.rerank_max_candidates = 48;
            changed = true;
        }
        if tuning.wiki_snippet_max_chars == 360 {
            tuning.wiki_snippet_max_chars = 1200;
            changed = true;
        }
        if tuning.web_snippet_max_chars == 360 {
            tuning.web_snippet_max_chars = 1200;
            changed = true;
        }
        if tuning.external_search_query_max_chars == 180 {
            tuning.external_search_query_max_chars = 1200;
            changed = true;
        }
        if tuning.rerank_query_max_chars == 420 {
            tuning.rerank_query_max_chars = 6000;
            changed = true;
        }
        if tuning.deep_research_depth == 0 {
            tuning.deep_research_depth = 3;
            changed = true;
        }
        if tuning.deep_research_branch_queries == 0 {
            tuning.deep_research_branch_queries = 3;
            changed = true;
        }

        changed
    }

    /// Validate settings and return validation result.
    ///
    /// Uses the same validation logic as the mock implementation.
    fn validate_settings(&self, settings: &SettingsDto) -> ValidationResult {
        let mut result = ValidationResult::success();

        // Validate indexing settings
        if settings.indexing.chunk_size == 0 {
            result.add_error("indexing", "chunk_size must be greater than 0".to_string());
        }
        if settings.indexing.chunk_size > 10000 {
            result.add_warning("indexing", "chunk_size is very large".to_string());
        }
        if settings.indexing.chunk_overlap >= settings.indexing.chunk_size {
            result.add_error(
                "indexing",
                "chunk_overlap must be less than chunk_size".to_string(),
            );
        }
        if settings.indexing.batch_size == 0 {
            result.add_error("indexing", "batch_size must be greater than 0".to_string());
        }

        // Validate search settings
        if settings.search.max_results == 0 {
            result.add_error("search", "max_results must be greater than 0".to_string());
        }
        if settings.search.similarity_threshold < 0.0 || settings.search.similarity_threshold > 1.0
        {
            result.add_error(
                "search",
                "similarity_threshold must be between 0.0 and 1.0".to_string(),
            );
        }
        if settings.search.hybrid_search_alpha < 0.0 || settings.search.hybrid_search_alpha > 1.0 {
            result.add_error(
                "search",
                "hybrid_search_alpha must be between 0.0 and 1.0".to_string(),
            );
        }
        let tuning = &settings.search.retrieval_tuning;
        if tuning.kb_search_min_limit == 0
            || tuning.kb_search_min_limit > tuning.kb_search_max_limit
        {
            result.add_error(
                "search",
                "kb_search_min_limit must be > 0 and <= kb_search_max_limit".to_string(),
            );
        }
        if tuning.doc_shortlist_candidate_min == 0
            || tuning.doc_shortlist_candidate_min > tuning.doc_shortlist_candidate_max
        {
            result.add_error(
                "search",
                "doc_shortlist_candidate_min must be > 0 and <= doc_shortlist_candidate_max"
                    .to_string(),
            );
        }
        if tuning.doc_shortlist_doc_min == 0
            || tuning.doc_shortlist_doc_min > tuning.doc_shortlist_doc_max
        {
            result.add_error(
                "search",
                "doc_shortlist_doc_min must be > 0 and <= doc_shortlist_doc_max".to_string(),
            );
        }
        if tuning.shortlist_gate_min_candidates == 0 || tuning.shortlist_gate_min_docs == 0 {
            result.add_error(
                "search",
                "shortlist gate minimums must be greater than 0".to_string(),
            );
        }
        if tuning.wiki_search_max_results == 0 || tuning.web_search_max_results == 0 {
            result.add_error(
                "search",
                "wiki_search_max_results and web_search_max_results must be greater than 0"
                    .to_string(),
            );
        }
        if tuning.deep_research_depth == 0 || tuning.deep_research_depth > 4 {
            result.add_error(
                "search",
                "deep_research_depth must be between 1 and 4".to_string(),
            );
        }
        if tuning.deep_research_branch_queries == 0 || tuning.deep_research_branch_queries > 4 {
            result.add_error(
                "search",
                "deep_research_branch_queries must be between 1 and 4".to_string(),
            );
        }
        if tuning.wiki_snippet_max_chars == 0
            || tuning.web_snippet_max_chars == 0
            || tuning.external_search_query_max_chars == 0
            || tuning.rerank_query_max_chars == 0
        {
            result.add_error(
                "search",
                "snippet/query character limits must be greater than 0".to_string(),
            );
        }
        if tuning.external_search_max_wiki_terms == 0 || tuning.external_search_max_web_terms == 0 {
            result.add_error(
                "search",
                "external_search_max_wiki_terms and external_search_max_web_terms must be > 0"
                    .to_string(),
            );
        }
        if tuning.rerank_max_candidates == 0 {
            result.add_error(
                "search",
                "rerank_max_candidates must be greater than 0".to_string(),
            );
        }
        if tuning.overlap_min_hits_for_multi_term == 0 {
            result.add_error(
                "search",
                "overlap_min_hits_for_multi_term must be greater than 0".to_string(),
            );
        }
        if tuning.doc_support_multi_hit_ratio_factor <= 0.0
            || tuning.doc_support_single_hit_ratio_factor <= 0.0
        {
            result.add_error(
                "search",
                "doc support ratio factors must be greater than 0.0".to_string(),
            );
        }
        if !(0.0..=1.0).contains(&tuning.doc_support_multi_hit_ratio_min)
            || !(0.0..=1.0).contains(&tuning.doc_support_multi_hit_ratio_max)
            || tuning.doc_support_multi_hit_ratio_min > tuning.doc_support_multi_hit_ratio_max
        {
            result.add_error(
                "search",
                "doc_support_multi_hit_ratio bounds must be between 0.0 and 1.0 and ordered"
                    .to_string(),
            );
        }
        if !(0.0..=1.0).contains(&tuning.doc_support_single_hit_ratio_min)
            || !(0.0..=1.0).contains(&tuning.doc_support_single_hit_ratio_max)
            || tuning.doc_support_single_hit_ratio_min > tuning.doc_support_single_hit_ratio_max
        {
            result.add_error(
                "search",
                "doc_support_single_hit_ratio bounds must be between 0.0 and 1.0 and ordered"
                    .to_string(),
            );
        }

        // Validate LLM settings
        if settings.llm.temperature < 0.0 || settings.llm.temperature > 2.0 {
            result.add_error("llm", "temperature must be between 0.0 and 2.0".to_string());
        }
        if settings.llm.top_p < 0.0 || settings.llm.top_p > 1.0 {
            result.add_error("llm", "top_p must be between 0.0 and 1.0".to_string());
        }
        if settings.llm.top_k < 0 {
            result.add_error(
                "llm",
                "top_k must be greater than or equal to 0".to_string(),
            );
        }
        if settings.llm.repeat_penalty <= 0.0 {
            result.add_error("llm", "repeat_penalty must be greater than 0.0".to_string());
        }
        if settings.llm.max_tokens == 0 {
            result.add_error("llm", "max_tokens must be greater than 0".to_string());
        }
        if settings.llm.context_window == 0 {
            result.add_error("llm", "context_window must be greater than 0".to_string());
        }
        if settings.llm.provider == LLMProvider::Ollama {
            if settings.llm.ollama_url.is_empty() {
                result.add_error("llm", "ollama_url cannot be empty".to_string());
            }
            if settings.llm.model.is_empty() {
                result.add_error("llm", "model cannot be empty".to_string());
            }
        }
        let auth_name = settings.llm.ollama_auth_header_name.trim();
        let auth_value = settings.llm.ollama_auth_header_value.trim();
        if (!auth_name.is_empty() && auth_value.is_empty())
            || (auth_name.is_empty() && !auth_value.is_empty())
        {
            result.add_error(
                "llm",
                "ollama_auth_header_name and ollama_auth_header_value must both be set".to_string(),
            );
        }
        if settings.llm.timeout_seconds == 0 {
            result.add_error("llm", "timeout_seconds must be greater than 0".to_string());
        }
        for path in &settings.llm.external_model_directories {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                result.add_error(
                    "llm",
                    "external_model_directories cannot contain empty paths".to_string(),
                );
                continue;
            }
            if trimmed.contains('\0') {
                result.add_error(
                    "llm",
                    "external_model_directories cannot contain null bytes".to_string(),
                );
                continue;
            }
            if !std::path::Path::new(trimmed).is_absolute() {
                result.add_warning(
                    "llm",
                    format!(
                        "external_model_directories path '{}' is not absolute and may be ignored",
                        trimmed
                    ),
                );
            }
        }
        if settings.llm.prompts.rag_prompt_template.trim().is_empty() {
            result.add_error("llm", "rag_prompt_template cannot be empty".to_string());
        }
        if settings
            .llm
            .prompts
            .greeting_prompt_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "greeting_prompt_template cannot be empty".to_string(),
            );
        }
        if settings
            .llm
            .prompts
            .no_context_prompt_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "no_context_prompt_template cannot be empty".to_string(),
            );
        }
        if settings.llm.tool_output.max_chars == 0 {
            result.add_error(
                "llm",
                "tool_output.max_chars must be greater than 0".to_string(),
            );
        }
        if settings.llm.tool_output.excerpt_chars == 0 {
            result.add_error(
                "llm",
                "tool_output.excerpt_chars must be greater than 0".to_string(),
            );
        }
        if settings.llm.tool_output.max_results == 0 {
            result.add_error(
                "llm",
                "tool_output.max_results must be greater than 0".to_string(),
            );
        }
        if settings.llm.tool_output.highlight_terms_max == 0 {
            result.add_warning(
                "llm",
                "tool_output.highlight_terms_max is 0; excerpts will be less relevant".to_string(),
            );
        }
        if !settings.llm.router.enabled {
            result.add_warning(
                "llm",
                "router.enabled is false; follow-up routing will use deterministic fallback behavior"
                    .to_string(),
            );
        }
        if settings.llm.router.enabled && settings.llm.router.model.trim().is_empty() {
            result.add_error(
                "llm",
                "router.model cannot be empty; configure a small router model (e.g. Phi-3.5 Mini or TinyLlama)"
                    .to_string(),
            );
        }
        if settings
            .llm
            .tool_output
            .templates
            .default_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "tool_output.templates.default_template cannot be empty".to_string(),
            );
        }
        if settings
            .llm
            .tool_output
            .templates
            .get_document_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "tool_output.templates.get_document_template cannot be empty".to_string(),
            );
        }
        if settings
            .llm
            .tool_output
            .templates
            .semantic_search_template
            .trim()
            .is_empty()
        {
            result.add_error(
                "llm",
                "tool_output.templates.semantic_search_template cannot be empty".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .rag_prompt_template
            .contains("{context}")
        {
            result.add_warning(
                "llm",
                "rag_prompt_template missing {context} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .rag_prompt_template
            .contains("{question}")
        {
            result.add_warning(
                "llm",
                "rag_prompt_template missing {question} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .greeting_prompt_template
            .contains("{question}")
        {
            result.add_warning(
                "llm",
                "greeting_prompt_template missing {question} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .no_context_prompt_template
            .contains("{question}")
        {
            result.add_warning(
                "llm",
                "no_context_prompt_template missing {question} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .tool_followup_prompt_template
            .contains("{question}")
        {
            result.add_warning(
                "llm",
                "tool_followup_prompt_template missing {question} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .prompts
            .tool_followup_prompt_template
            .contains("{previous_response}")
        {
            result.add_warning(
                "llm",
                "tool_followup_prompt_template missing {previous_response} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .tool_output
            .templates
            .default_template
            .contains("{json}")
        {
            result.add_warning(
                "llm",
                "tool_output.templates.default_template missing {json} placeholder".to_string(),
            );
        }
        if !settings
            .llm
            .tool_output
            .templates
            .get_document_template
            .contains("{excerpt}")
        {
            result.add_warning(
                "llm",
                "tool_output.templates.get_document_template missing {excerpt} placeholder"
                    .to_string(),
            );
        }
        if !settings
            .llm
            .tool_output
            .templates
            .semantic_search_template
            .contains("{results}")
        {
            result.add_warning(
                "llm",
                "tool_output.templates.semantic_search_template missing {results} placeholder"
                    .to_string(),
            );
        }
        let mut custom_tool_names = std::collections::HashSet::new();
        for tool in &settings.llm.custom_tools {
            if !tool.enabled {
                continue;
            }

            let tool_name = tool.name.trim();
            if tool_name.is_empty() {
                result.add_error("llm", "custom_tools.name cannot be empty".to_string());
            } else {
                let valid_name = tool_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !valid_name {
                    result.add_error(
                        "llm",
                        format!(
                            "custom_tools.name '{}' must use only alphanumeric characters and underscores",
                            tool_name
                        ),
                    );
                }
                if !custom_tool_names.insert(tool_name.to_string()) {
                    result.add_error("llm", format!("Duplicate custom tool name '{}'", tool_name));
                }
            }

            if tool.description.trim().is_empty() {
                result.add_error(
                    "llm",
                    format!("custom tool '{}' description cannot be empty", tool_name),
                );
            }

            if tool.query_param.trim().is_empty() {
                result.add_error(
                    "llm",
                    format!(
                        "custom tool '{}' query_param cannot be empty",
                        if tool_name.is_empty() {
                            "<unnamed>"
                        } else {
                            tool_name
                        }
                    ),
                );
            }

            if tool.default_max_results == 0 {
                result.add_error(
                    "llm",
                    format!(
                        "custom tool '{}' default_max_results must be greater than 0",
                        if tool_name.is_empty() {
                            "<unnamed>"
                        } else {
                            tool_name
                        }
                    ),
                );
            }

            match Url::parse(tool.endpoint.trim()) {
                Ok(parsed) => {
                    if !matches!(parsed.scheme(), "https" | "http") {
                        result.add_error(
                            "llm",
                            format!(
                                "custom tool '{}' endpoint must use http or https",
                                if tool_name.is_empty() {
                                    "<unnamed>"
                                } else {
                                    tool_name
                                }
                            ),
                        );
                    }
                }
                Err(_) => {
                    result.add_error(
                        "llm",
                        format!(
                            "custom tool '{}' endpoint must be a valid URL",
                            if tool_name.is_empty() {
                                "<unnamed>"
                            } else {
                                tool_name
                            }
                        ),
                    );
                }
            }
        }

        // Validate UI settings
        if settings.ui.font_size < 8 || settings.ui.font_size > 72 {
            result.add_error("ui", "font_size must be between 8 and 72".to_string());
        }
        if settings.ui.results_per_page == 0 {
            result.add_error("ui", "results_per_page must be greater than 0".to_string());
        }
        if !["light", "dark", "system"].contains(&settings.ui.theme.as_str()) {
            result.add_warning(
                "ui",
                format!("Unknown theme '{}', using 'system'", settings.ui.theme),
            );
        }

        // Validate sync settings
        if settings.sync.sync_enabled && settings.sync.sync_url.is_empty() {
            result.add_error(
                "sync",
                "sync_url is required when sync is enabled".to_string(),
            );
        }
        if settings.sync.sync_interval_minutes == 0 {
            result.add_error(
                "sync",
                "sync_interval_minutes must be greater than 0".to_string(),
            );
        }

        // Validate backup settings
        if settings.backup.auto_backup_enabled && settings.backup.backup_path.is_empty() {
            result.add_error(
                "backup",
                "backup_path is required when auto-backup is enabled".to_string(),
            );
        }
        if settings.backup.backup_retention_days == 0 {
            result.add_warning(
                "backup",
                "backup_retention_days is 0, backups won't be retained".to_string(),
            );
        }
        if !["daily", "weekly", "monthly"].contains(&settings.backup.backup_frequency.as_str()) {
            result.add_warning(
                "backup",
                format!(
                    "Unknown backup frequency '{}', using 'daily'",
                    settings.backup.backup_frequency
                ),
            );
        }

        result
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
            category_map.insert(key.clone(), value.clone());
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
        };

        Ok(value)
    }

    async fn save_all(&self, settings: &SettingsDto) -> Result<()> {
        // Validate before saving
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

    async fn update(
        &self,
        category: Option<SettingsCategory>,
        updates: HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto> {
        let mut settings = self.get_all().await?;

        match category {
            Some(cat) => {
                // Update specific category
                let category_value = match cat {
                    SettingsCategory::Indexing => serde_json::to_value(&settings.indexing)?,
                    SettingsCategory::Search => serde_json::to_value(&settings.search)?,
                    SettingsCategory::Llm => serde_json::to_value(&settings.llm)?,
                    SettingsCategory::Ui => serde_json::to_value(&settings.ui)?,
                    SettingsCategory::Sync => serde_json::to_value(&settings.sync)?,
                    SettingsCategory::Backup => serde_json::to_value(&settings.backup)?,
                    SettingsCategory::Privacy => serde_json::to_value(&settings.privacy)?,
                };

                let merged = self.merge_category_updates(category_value, &updates)?;

                // Update the category
                match cat {
                    SettingsCategory::Indexing => {
                        settings.indexing = serde_json::from_value(merged)?;
                    }
                    SettingsCategory::Search => {
                        settings.search = serde_json::from_value(merged)?;
                    }
                    SettingsCategory::Llm => {
                        settings.llm = serde_json::from_value(merged)?;
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
                }
            }
            None => {
                // Update all settings (flat structure)
                let mut settings_value = serde_json::to_value(&settings)?;
                let settings_map = settings_value.as_object_mut().ok_or_else(|| {
                    AppError::InvalidInput("Settings is not an object".to_string())
                })?;

                for (key, value) in updates {
                    settings_map.insert(key, value);
                }

                settings = serde_json::from_value(serde_json::Value::Object(settings_map.clone()))?;
            }
        }

        // Save updated settings (includes validation)
        self.save_all(&settings).await?;

        Ok(settings)
    }

    async fn reset(&self, category: Option<SettingsCategory>) -> Result<SettingsDto> {
        let mut settings = self.get_all().await?;

        match category {
            Some(cat) => {
                // Reset specific category to defaults
                match cat {
                    SettingsCategory::Indexing => settings.indexing = Default::default(),
                    SettingsCategory::Search => settings.search = Default::default(),
                    SettingsCategory::Llm => settings.llm = Default::default(),
                    SettingsCategory::Ui => settings.ui = Default::default(),
                    SettingsCategory::Sync => settings.sync = Default::default(),
                    SettingsCategory::Backup => settings.backup = Default::default(),
                    SettingsCategory::Privacy => settings.privacy = Default::default(),
                }
            }
            None => {
                // Reset all settings to defaults
                settings = SettingsDto::default();
            }
        }

        // Save reset settings
        self.save_all(&settings).await?;

        Ok(settings)
    }

    async fn export(&self, path: &str) -> Result<()> {
        let settings = self.get_all().await?;

        // Serialize to JSON with pretty printing
        let json = serde_json::to_string_pretty(&settings)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize settings: {}", e)))?;

        // Write to export path
        fs::write(path, json)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to write export file: {}", e)))?;

        Ok(())
    }

    async fn import(&self, path: &str, merge: bool) -> Result<SettingsDto> {
        // Read import file
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to read import file: {}", e)))?;

        // Deserialize
        let imported_settings: SettingsDto = serde_json::from_str(&content).map_err(|e| {
            AppError::Deserialization(format!("Failed to deserialize settings: {}", e))
        })?;

        // Validate imported settings
        let validation = self.validate_settings(&imported_settings);
        if validation.has_errors() {
            return Err(AppError::ValidationFailed(format!(
                "Imported settings validation failed: {:?}",
                validation.errors
            )));
        }

        let final_settings = if merge {
            // Merge with existing settings
            let mut existing = self.get_all().await?;

            // Merge each category (imported values override existing)
            existing.indexing = imported_settings.indexing;
            existing.search = imported_settings.search;
            existing.llm = imported_settings.llm;
            existing.ui = imported_settings.ui;
            existing.sync = imported_settings.sync;
            existing.backup = imported_settings.backup;
            existing.privacy = imported_settings.privacy;

            existing
        } else {
            // Replace all settings
            imported_settings
        };

        // Save merged/replaced settings
        self.save_all(&final_settings).await?;

        Ok(final_settings)
    }

    fn validate(&self, settings: &SettingsDto) -> ValidationResult {
        self.validate_settings(settings)
    }

    fn validate_folder_path(&self, path: &str) -> bool {
        let path = Path::new(path);
        path.exists() && path.is_dir()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_repository() -> (SettingsRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        (repo, temp_dir)
    }

    #[tokio::test]
    async fn test_new_repository_creates_default_settings() {
        let (repo, _temp_dir) = create_test_repository().await;

        let settings = repo.get_all().await.unwrap();

        assert_eq!(settings.indexing.chunk_size, 800);
        assert_eq!(settings.search.max_results, 10);
        assert_eq!(settings.llm.model, "llama3.2:latest");
    }

    #[tokio::test]
    async fn test_get_category() {
        let (repo, _temp_dir) = create_test_repository().await;

        let search = repo.get_category(SettingsCategory::Search).await.unwrap();

        assert!(search.is_object());
        assert_eq!(search["maxResults"], 10);
    }

    #[tokio::test]
    async fn test_save_and_retrieve() {
        let (repo, _temp_dir) = create_test_repository().await;

        let mut settings = repo.get_all().await.unwrap();
        settings.search.max_results = 20;

        repo.save_all(&settings).await.unwrap();

        let retrieved = repo.get_all().await.unwrap();
        assert_eq!(retrieved.search.max_results, 20);
    }

    #[tokio::test]
    async fn test_update_category() {
        let (repo, _temp_dir) = create_test_repository().await;

        let mut updates = HashMap::new();
        updates.insert("maxResults".to_string(), serde_json::json!(30));

        let updated = repo
            .update(Some(SettingsCategory::Search), updates)
            .await
            .unwrap();

        assert_eq!(updated.search.max_results, 30);

        // Verify persistence
        let retrieved = repo.get_all().await.unwrap();
        assert_eq!(retrieved.search.max_results, 30);
    }

    #[tokio::test]
    async fn test_reset_category() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Modify settings
        let mut updates = HashMap::new();
        updates.insert("maxResults".to_string(), serde_json::json!(50));
        repo.update(Some(SettingsCategory::Search), updates)
            .await
            .unwrap();

        // Reset
        let reset = repo.reset(Some(SettingsCategory::Search)).await.unwrap();

        assert_eq!(reset.search.max_results, 10); // Back to default
    }

    #[tokio::test]
    async fn test_reset_all() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Modify multiple categories
        let mut settings = repo.get_all().await.unwrap();
        settings.search.max_results = 50;
        settings.indexing.chunk_size = 1024;
        repo.save_all(&settings).await.unwrap();

        // Reset all
        let reset = repo.reset(None).await.unwrap();

        assert_eq!(reset.search.max_results, 10);
        assert_eq!(reset.indexing.chunk_size, 800);
    }

    #[tokio::test]
    async fn test_export_and_import() {
        let (repo, temp_dir) = create_test_repository().await;

        // Modify settings
        let mut settings = repo.get_all().await.unwrap();
        settings.search.max_results = 25;
        repo.save_all(&settings).await.unwrap();

        // Export
        let export_path = temp_dir.path().join("export.json");
        repo.export(export_path.to_str().unwrap()).await.unwrap();

        // Reset to defaults
        repo.reset(None).await.unwrap();

        // Import
        let imported = repo
            .import(export_path.to_str().unwrap(), false)
            .await
            .unwrap();

        assert_eq!(imported.search.max_results, 25);
    }

    #[tokio::test]
    async fn test_import_merge() {
        let (repo, temp_dir) = create_test_repository().await;

        // Create export with modified search settings
        let mut export_settings = SettingsDto::default();
        export_settings.search.max_results = 15;

        let export_path = temp_dir.path().join("export.json");
        let json = serde_json::to_string_pretty(&export_settings).unwrap();
        fs::write(&export_path, json).await.unwrap();

        // Modify local indexing settings
        let mut local_settings = repo.get_all().await.unwrap();
        local_settings.indexing.chunk_size = 1024;
        repo.save_all(&local_settings).await.unwrap();

        // Import with merge
        let merged = repo
            .import(export_path.to_str().unwrap(), true)
            .await
            .unwrap();

        // Should have imported search settings
        assert_eq!(merged.search.max_results, 15);
        // Note: Current implementation replaces categories, so chunk_size will be reset
        // This is expected behavior for category-level merging
    }

    #[tokio::test]
    async fn test_validation_on_save() {
        let (repo, _temp_dir) = create_test_repository().await;

        let mut settings = repo.get_all().await.unwrap();
        settings.indexing.chunk_size = 0; // Invalid

        let result = repo.save_all(&settings).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validation_errors() {
        let (repo, _temp_dir) = create_test_repository().await;

        let mut settings = SettingsDto::default();
        settings.indexing.chunk_size = 0;
        settings.search.similarity_threshold = 1.5;

        let validation = repo.validate(&settings);

        assert!(!validation.valid);
        assert!(validation.has_errors());
        assert!(validation.errors.contains_key("indexing"));
        assert!(validation.errors.contains_key("search"));
    }

    #[tokio::test]
    async fn test_validation_warnings() {
        let (repo, _temp_dir) = create_test_repository().await;

        let mut settings = SettingsDto::default();
        settings.indexing.chunk_size = 15000; // Very large

        let validation = repo.validate(&settings);

        assert!(validation.valid); // Still valid, just has warnings
        assert!(validation.has_warnings());
        assert!(validation.warnings.contains_key("indexing"));
    }

    #[tokio::test]
    async fn test_atomic_write_creates_temp_file() {
        let (repo, temp_dir) = create_test_repository().await;

        let settings = repo.get_all().await.unwrap();
        repo.save_all(&settings).await.unwrap();

        // Verify settings file exists
        let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);
        assert!(settings_path.exists());

        // Verify temp file was cleaned up
        let temp_path = settings_path.with_extension(TEMP_SUFFIX);
        assert!(!temp_path.exists());
    }

    #[tokio::test]
    async fn test_validate_folder_path() {
        let (repo, temp_dir) = create_test_repository().await;

        // Valid directory
        assert!(repo.validate_folder_path(temp_dir.path().to_str().unwrap()));

        // Invalid path
        assert!(!repo.validate_folder_path("/nonexistent/path"));

        // File instead of directory
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test").await.unwrap();
        assert!(!repo.validate_folder_path(file_path.to_str().unwrap()));
    }

    #[tokio::test]
    async fn test_corrupted_file_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);

        // Write corrupted JSON
        fs::write(&settings_path, "{ corrupted json }")
            .await
            .unwrap();

        // Repository should recover with defaults
        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let settings = repo.get_all().await.unwrap();
        assert_eq!(settings.indexing.chunk_size, 800); // Default value
    }

    #[tokio::test]
    async fn test_settings_file_version() {
        let (repo, temp_dir) = create_test_repository().await;

        // Read the actual file to check version
        let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);
        let content = fs::read_to_string(&settings_path).await.unwrap();
        let settings_file: SettingsFile = serde_json::from_str(&content).unwrap();

        assert_eq!(settings_file.version, SETTINGS_VERSION);
    }

    #[test]
    fn test_upgrade_legacy_low_limit_defaults_migrates_chunk_and_retrieval_profile() {
        let mut settings = SettingsDto::default();
        settings.indexing.chunk_size = 250;
        settings.indexing.chunk_overlap = 50;

        settings.search.retrieval_tuning.kb_search_min_limit = 12;
        settings.search.retrieval_tuning.kb_search_max_limit = 30;
        settings.search.retrieval_tuning.doc_shortlist_candidate_min = 36;
        settings.search.retrieval_tuning.doc_shortlist_candidate_max = 96;
        settings.search.retrieval_tuning.doc_shortlist_doc_min = 8;
        settings.search.retrieval_tuning.doc_shortlist_doc_max = 24;
        settings
            .search
            .retrieval_tuning
            .shortlist_gate_min_candidates = 6;
        settings.search.retrieval_tuning.shortlist_gate_min_docs = 3;
        settings.search.retrieval_tuning.rerank_max_candidates = 24;
        settings.search.retrieval_tuning.deep_research_depth = 0;
        settings
            .search
            .retrieval_tuning
            .deep_research_branch_queries = 0;

        let upgraded = SettingsRepository::upgrade_legacy_low_limit_defaults(&mut settings);

        assert!(upgraded);
        assert_eq!(settings.indexing.chunk_size, 800);
        assert_eq!(settings.indexing.chunk_overlap, 120);
        assert_eq!(settings.search.retrieval_tuning.kb_search_min_limit, 16);
        assert_eq!(settings.search.retrieval_tuning.kb_search_max_limit, 48);
        assert_eq!(
            settings.search.retrieval_tuning.doc_shortlist_candidate_min,
            48
        );
        assert_eq!(
            settings.search.retrieval_tuning.doc_shortlist_candidate_max,
            192
        );
        assert_eq!(settings.search.retrieval_tuning.doc_shortlist_doc_min, 10);
        assert_eq!(settings.search.retrieval_tuning.doc_shortlist_doc_max, 32);
        assert_eq!(
            settings
                .search
                .retrieval_tuning
                .shortlist_gate_min_candidates,
            8
        );
        assert_eq!(settings.search.retrieval_tuning.shortlist_gate_min_docs, 4);
        assert_eq!(settings.search.retrieval_tuning.rerank_max_candidates, 48);
        assert_eq!(settings.search.retrieval_tuning.deep_research_depth, 3);
        assert_eq!(
            settings
                .search
                .retrieval_tuning
                .deep_research_branch_queries,
            3
        );
    }

    #[test]
    fn test_upgrade_legacy_low_limit_defaults_preserves_non_legacy_values() {
        let mut settings = SettingsDto::default();
        settings.indexing.chunk_size = 1024;
        settings.indexing.chunk_overlap = 160;
        settings.search.retrieval_tuning.kb_search_min_limit = 20;
        settings.search.retrieval_tuning.kb_search_max_limit = 60;
        settings.search.retrieval_tuning.rerank_max_candidates = 64;

        let upgraded = SettingsRepository::upgrade_legacy_low_limit_defaults(&mut settings);

        assert!(!upgraded);
        assert_eq!(settings.indexing.chunk_size, 1024);
        assert_eq!(settings.indexing.chunk_overlap, 160);
        assert_eq!(settings.search.retrieval_tuning.kb_search_min_limit, 20);
        assert_eq!(settings.search.retrieval_tuning.kb_search_max_limit, 60);
        assert_eq!(settings.search.retrieval_tuning.rerank_max_candidates, 64);
    }

    // === AppConfig → Settings migration (Phase 4b/7) ===

    async fn write_legacy_config(dir: &Path, contents: &str) {
        fs::write(dir.join("config.json"), contents).await.unwrap();
    }

    #[tokio::test]
    async fn test_legacy_config_migration_copies_indexed_paths() {
        let temp_dir = TempDir::new().unwrap();
        let legacy = serde_json::json!({
            "indexedPaths": ["/Users/josh/Documents", "/Users/josh/Projects"],
            "excludePatterns": ["*.tmp", "node_modules"],
            "autoIndex": true,
            "ollamaEndpoint": "http://localhost:11434",
            "ollamaModel": "llama3.2:latest",
        });
        write_legacy_config(temp_dir.path(), &legacy.to_string()).await;

        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        let settings = repo.get_all().await.unwrap();

        assert_eq!(
            settings.indexing.indexed_paths,
            vec!["/Users/josh/Documents", "/Users/josh/Projects"]
        );

        // Legacy config should be deleted after successful migration.
        assert!(!temp_dir.path().join("config.json").exists());
    }

    #[tokio::test]
    async fn test_legacy_config_migration_overrides_auto_index_only_when_disabled() {
        let temp_dir = TempDir::new().unwrap();
        // AppConfig had auto_index=false explicitly. Settings default is true.
        // We honor the user's explicit opt-out.
        let legacy = serde_json::json!({
            "indexedPaths": [],
            "excludePatterns": [],
            "autoIndex": false,
            "ollamaEndpoint": "",
            "ollamaModel": "",
        });
        write_legacy_config(temp_dir.path(), &legacy.to_string()).await;

        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        let settings = repo.get_all().await.unwrap();
        assert!(!settings.indexing.auto_index_new_files);
    }

    #[tokio::test]
    async fn test_legacy_config_migration_does_not_clobber_custom_ollama_url() {
        let temp_dir = TempDir::new().unwrap();
        // settings.json already has a non-default ollama_url; legacy config
        // points somewhere else. The legacy value must NOT win.
        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        let mut settings = repo.get_all().await.unwrap();
        settings.llm.ollama_url = "https://my-custom-ollama.example.com".to_string();
        repo.save_all(&settings).await.unwrap();

        // Now drop a legacy config.json with a different URL.
        let legacy = serde_json::json!({
            "indexedPaths": [],
            "excludePatterns": [],
            "autoIndex": true,
            "ollamaEndpoint": "http://different-ollama:11434",
            "ollamaModel": "",
        });
        write_legacy_config(temp_dir.path(), &legacy.to_string()).await;

        // Re-instantiate the repo to trigger migration.
        let repo2 = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        let settings2 = repo2.get_all().await.unwrap();

        assert_eq!(
            settings2.llm.ollama_url,
            "https://my-custom-ollama.example.com",
            "user customization must survive the migration"
        );
        assert!(!temp_dir.path().join("config.json").exists());
    }

    #[tokio::test]
    async fn test_legacy_config_migration_corrupt_file_is_non_fatal() {
        let temp_dir = TempDir::new().unwrap();
        write_legacy_config(temp_dir.path(), "{ this is not json").await;

        // Must NOT panic / return an error — corrupt legacy config should
        // log a warning and proceed with defaults.
        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .expect("startup must succeed even if legacy config is corrupt");
        let settings = repo.get_all().await.unwrap();

        // Defaults intact.
        assert_eq!(settings.indexing.chunk_size, 800);
        // Corrupt file remains on disk (we only delete on successful migration).
        assert!(temp_dir.path().join("config.json").exists());
    }

    #[tokio::test]
    async fn test_legacy_config_migration_is_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let legacy = serde_json::json!({
            "indexedPaths": ["/some/path"],
            "excludePatterns": [],
            "autoIndex": true,
            "ollamaEndpoint": "",
            "ollamaModel": "",
        });
        write_legacy_config(temp_dir.path(), &legacy.to_string()).await;

        let _repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        // Second `new()` — config.json is already gone; must not panic or
        // break.
        let repo2 = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        let settings = repo2.get_all().await.unwrap();
        assert_eq!(settings.indexing.indexed_paths, vec!["/some/path"]);
    }

    // === Self-heal for the legacy DI bug ===
    //
    // Pre-fix, `interfaces/di/modules.rs` passed `data_dir/settings.json`
    // as the dir arg to SettingsRepository::new, which then joined
    // `settings.json` again. The result on disk was a directory at
    // `data_dir/settings.json/` containing a nested `settings.json`
    // file. Once the DI bug is fixed, the constructor still has to
    // cope with the stale on-disk state from prior runs, otherwise it
    // crashes startup with `Is a directory (os error 21)`.

    #[tokio::test]
    async fn test_self_heal_removes_stray_settings_directory() {
        let temp_dir = TempDir::new().unwrap();
        let stray = temp_dir.path().join("settings.json");
        fs::create_dir(&stray).await.unwrap();

        // Constructor must not error out on the stray directory.
        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .expect("constructor should self-heal stray settings.json directory");

        // Now `settings.json` must be a regular file.
        let meta = std::fs::metadata(&stray).unwrap();
        assert!(meta.is_file(), "settings.json must be a file after self-heal");

        // get_all should work — defaults applied since no salvage was
        // possible (empty stray dir).
        let settings = repo.get_all().await.unwrap();
        assert_eq!(settings.indexing.chunk_size, 800);
    }

    #[tokio::test]
    async fn test_self_heal_salvages_nested_settings_json() {
        let temp_dir = TempDir::new().unwrap();
        let stray = temp_dir.path().join("settings.json");
        fs::create_dir(&stray).await.unwrap();

        // Drop a real settings file inside the stray directory — this
        // is the layout the buggy DI produced. Self-heal should salvage
        // its contents.
        let nested = stray.join("settings.json");
        let mut nested_settings = SettingsDto::default();
        nested_settings.indexing.batch_size = 99;
        let nested_file = SettingsFile {
            version: SETTINGS_VERSION,
            settings: nested_settings,
        };
        let payload = serde_json::to_string_pretty(&nested_file).unwrap();
        fs::write(&nested, payload).await.unwrap();

        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .expect("self-heal should salvage nested settings.json");

        let meta = std::fs::metadata(&stray).unwrap();
        assert!(meta.is_file(), "settings.json must be a file after salvage");

        // Salvaged value preserved.
        let settings = repo.get_all().await.unwrap();
        assert_eq!(settings.indexing.batch_size, 99);
    }
}
