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

use crate::application::ports::{merge_json_update, SettingsRepositoryPort};
use crate::features::settings::dto::{
    LLMProvider, SettingsCategory, SettingsDto, ValidationResult, VectorIndexCompressionModeDto,
    MAX_VECTOR_COMPRESSION_DIMS, MIN_VECTOR_COMPRESSION_DIMS,
};
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
/// Multiple concurrent reads are safe. `update` and `reset` hold
/// `write_lock` across their whole read-merge-write cycle, so concurrent
/// updates to different categories cannot lose each other. Each write goes
/// to a uniquely-named temp file before an atomic rename, so a write that
/// is interrupted never leaves partial JSON in place.
///
/// Note that `save_all` on its own is *not* serialized against `update` —
/// it is a whole-document overwrite, and the caller is responsible for
/// having read the state it is overwriting.
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
    /// Serializes the read-modify-write cycle in `update` / `reset`.
    ///
    /// Those operations are `get_all()` → merge one category → `save_all()`.
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

    /// Read settings file from disk.
    ///
    /// If the file is corrupted or has invalid format, returns default settings
    /// and overwrites the corrupted file.
    async fn read_settings_file(&self) -> Result<SettingsFile> {
        match fs::read_to_string(&self.settings_path).await {
            Ok(content) => {
                match serde_json::from_str::<SettingsFile>(&content) {
                    Ok(settings_file) => {
                        if settings_file.version < SETTINGS_VERSION {
                            self.migrate_settings(settings_file).await
                        } else {
                            Ok(settings_file)
                        }
                    }
                    Err(e) => {
                        // Preserve the unparseable file before falling back —
                        // it is the only copy of the user's configuration.
                        self.quarantine_corrupt_settings(&e.to_string()).await;
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
    ///
    /// The temp filename is unique per write. A single shared `settings.tmp`
    /// is not safe: `File::create` truncates, so a second writer can truncate
    /// and rewrite the temp file while the first sits between `sync_all` and
    /// `rename`, renaming half-written JSON into place. That torn file then
    /// fails to parse on the next read.
    async fn write_settings_file(&self, settings_file: &SettingsFile) -> Result<()> {
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
    ///
    /// Currently a no-op as we're on version 1.
    /// Future versions should implement migration logic here.
    async fn migrate_settings(&self, mut settings_file: SettingsFile) -> Result<SettingsFile> {
        tracing::info!(
            "Migrating settings from version {} to {}",
            settings_file.version,
            SETTINGS_VERSION
        );

        settings_file.version = SETTINGS_VERSION;

        self.write_settings_file(&settings_file).await?;

        Ok(settings_file)
    }

    /// Validate settings and return validation result.
    ///
    /// Uses the same validation logic as the mock implementation.
    fn validate_settings(&self, settings: &SettingsDto) -> ValidationResult {
        let mut result = ValidationResult::success();

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
        // Only checked when truncation is on: the numbers are remembered while
        // the mode is `none`, and rejecting a remembered value the index never
        // reads would make the whole settings document unsaveable.
        if settings.search.vector_index_compression.mode == VectorIndexCompressionModeDto::Truncated
        {
            let dims = settings.search.vector_index_compression.dims;
            if !(MIN_VECTOR_COMPRESSION_DIMS..=MAX_VECTOR_COMPRESSION_DIMS).contains(&dims) {
                result.add_error(
                    "search",
                    format!(
                        "vector_index_compression.dims must be between {} and {}",
                        MIN_VECTOR_COMPRESSION_DIMS, MAX_VECTOR_COMPRESSION_DIMS
                    ),
                );
            }
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
        if !(0.0..=1.0).contains(&tuning.sufficiency_min_top_score)
            || !(0.0..=1.0).contains(&tuning.sufficiency_min_term_coverage)
        {
            result.add_error(
                "search",
                "sufficiency thresholds must be between 0.0 and 1.0".to_string(),
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
        if let Err(error) =
            crate::features::llm::llama_cpp::validate_connection(&settings.llm.llama_cpp)
        {
            result.add_error("llm", error.to_string());
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

        // Validate backup settings.
        // `backup_path` is intentionally empty: scheduled backups always use
        // the application-owned backups directory. The field is retained for
        // settings-file compatibility, but accepting an arbitrary value here
        // would only defer a guaranteed adapter rejection until the scheduler
        // runs. Reject it while saving so the failure is immediate and visible.
        if !settings.backup.backup_path.is_empty() {
            result.add_error(
                "backup",
                "custom backup locations are not supported; leave backup_path empty to use the application's protected backups directory".to_string(),
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

        self.save_all(&settings).await?;

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

        self.save_all(&settings).await?;

        Ok(settings)
    }

    async fn export(&self, path: &str) -> Result<()> {
        let settings = self.get_all().await?;

        // Serialize to JSON with pretty printing
        let json = serde_json::to_string_pretty(&settings)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize settings: {}", e)))?;

        fs::write(path, json)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to write export file: {}", e)))?;

        Ok(())
    }

    async fn import(&self, path: &str, merge: bool) -> Result<SettingsDto> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| AppError::Storage(format!("Failed to read import file: {}", e)))?;

        // Deserialize
        let imported_settings: SettingsDto = serde_json::from_str(&content).map_err(|e| {
            AppError::Deserialization(format!("Failed to deserialize settings: {}", e))
        })?;

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

        self.save_all(&final_settings).await?;

        Ok(final_settings)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn create_test_repository() -> (SettingsRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        (repo, temp_dir)
    }

    /// Concurrent updates to *different* categories must all survive. Before
    /// the write lock, each caller read the same base state and the last
    /// writer reverted every earlier one, while the UI reported success for
    /// all of them.
    #[tokio::test]
    async fn concurrent_updates_to_distinct_categories_do_not_lose_writes() {
        let temp_dir = TempDir::new().unwrap();
        let repo = Arc::new(
            SettingsRepository::new(temp_dir.path().to_path_buf())
                .await
                .unwrap(),
        );

        let mut handles = Vec::new();

        {
            let repo = Arc::clone(&repo);
            handles.push(tokio::spawn(async move {
                let mut updates = HashMap::new();
                updates.insert("maxResults".to_string(), serde_json::json!(42));
                repo.update(Some(SettingsCategory::Search), updates).await
            }));
        }
        {
            let repo = Arc::clone(&repo);
            handles.push(tokio::spawn(async move {
                let mut updates = HashMap::new();
                updates.insert("chunkSize".to_string(), serde_json::json!(1234));
                repo.update(Some(SettingsCategory::Indexing), updates).await
            }));
        }
        {
            let repo = Arc::clone(&repo);
            handles.push(tokio::spawn(async move {
                let mut updates = HashMap::new();
                updates.insert("telemetryEnabled".to_string(), serde_json::json!(true));
                repo.update(Some(SettingsCategory::Privacy), updates).await
            }));
        }

        for handle in handles {
            handle.await.unwrap().expect("update should succeed");
        }

        let settings = repo.get_all().await.unwrap();
        assert_eq!(settings.search.max_results, 42, "search update was lost");
        assert_eq!(
            settings.indexing.chunk_size, 1234,
            "indexing update was lost"
        );
        assert!(
            settings.privacy.telemetry_enabled,
            "privacy update was lost"
        );
    }

    /// An unparseable settings file must be preserved, not overwritten. It is
    /// the only copy of the user's vault path, indexed folders and model roles.
    #[tokio::test]
    async fn corrupt_settings_file_is_quarantined_not_destroyed() {
        let temp_dir = TempDir::new().unwrap();
        let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);

        let garbage = "{ this is not valid json";
        fs::write(&settings_path, garbage).await.unwrap();

        let repo = SettingsRepository::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        // Reading falls back to defaults...
        let settings = repo.get_all().await.unwrap();
        assert_eq!(settings.indexing.chunk_size, 800);

        // ...but the original bytes still exist somewhere.
        let mut preserved = Vec::new();
        let mut entries = tokio::fs::read_dir(temp_dir.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(".corrupt-") {
                preserved.push(tokio::fs::read_to_string(entry.path()).await.unwrap());
            }
        }

        assert_eq!(
            preserved.len(),
            1,
            "expected exactly one quarantined settings file"
        );
        assert_eq!(
            preserved[0], garbage,
            "quarantined file must hold the original bytes verbatim"
        );
    }

    /// Every write must use its own temp file, so a concurrent writer can't
    /// truncate the file another writer is about to rename into place.
    #[tokio::test]
    async fn concurrent_saves_never_leave_unparseable_settings() {
        let temp_dir = TempDir::new().unwrap();
        let repo = Arc::new(
            SettingsRepository::new(temp_dir.path().to_path_buf())
                .await
                .unwrap(),
        );
        let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);

        let mut handles = Vec::new();
        for i in 0..16 {
            let repo = Arc::clone(&repo);
            handles.push(tokio::spawn(async move {
                let mut updates = HashMap::new();
                updates.insert("maxResults".to_string(), serde_json::json!(i + 1));
                repo.update(Some(SettingsCategory::Search), updates).await
            }));
        }
        for handle in handles {
            handle.await.unwrap().expect("update should succeed");
        }

        let contents = fs::read_to_string(&settings_path).await.unwrap();
        serde_json::from_str::<SettingsFile>(&contents)
            .expect("settings file must remain parseable after concurrent writes");

        // And no temp files should be left lying around.
        let mut leftovers = Vec::new();
        let mut entries = tokio::fs::read_dir(temp_dir.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(TEMP_SUFFIX) {
                leftovers.push(name);
            }
        }
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {:?}",
            leftovers
        );
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

        let export_path = temp_dir.path().join("export.json");
        repo.export(export_path.to_str().unwrap()).await.unwrap();

        repo.reset(None).await.unwrap();

        let imported = repo
            .import(export_path.to_str().unwrap(), false)
            .await
            .unwrap();

        assert_eq!(imported.search.max_results, 25);
    }

    #[tokio::test]
    async fn test_import_merge() {
        let (repo, temp_dir) = create_test_repository().await;

        let mut export_settings = SettingsDto::default();
        export_settings.search.max_results = 15;

        let export_path = temp_dir.path().join("export.json");
        let json = serde_json::to_string_pretty(&export_settings).unwrap();
        fs::write(&export_path, json).await.unwrap();

        // Modify local indexing settings
        let mut local_settings = repo.get_all().await.unwrap();
        local_settings.indexing.chunk_size = 1024;
        repo.save_all(&local_settings).await.unwrap();

        let merged = repo
            .import(export_path.to_str().unwrap(), true)
            .await
            .unwrap();

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

        let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);
        assert!(settings_path.exists());

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
        let (_repo, temp_dir) = create_test_repository().await;

        let settings_path = temp_dir.path().join(SETTINGS_FILE_NAME);
        let content = fs::read_to_string(&settings_path).await.unwrap();
        let settings_file: SettingsFile = serde_json::from_str(&content).unwrap();

        assert_eq!(settings_file.version, SETTINGS_VERSION);
    }

    /// The dims are remembered while the mode is `none`, so validation only
    /// reads them when truncation is actually on. Rejecting a remembered value
    /// nothing consumes would make the whole settings document unsaveable.
    #[tokio::test]
    async fn vector_compression_dims_are_only_checked_when_truncation_is_on() {
        let (repo, _dir) = create_test_repository().await;

        let mut settings = SettingsDto::default();
        settings.search.vector_index_compression.mode = VectorIndexCompressionModeDto::None;
        settings.search.vector_index_compression.dims = 1;
        assert!(repo.validate_settings(&settings).valid);

        settings.search.vector_index_compression.mode = VectorIndexCompressionModeDto::Truncated;
        let rejected = repo.validate_settings(&settings);
        assert!(!rejected.valid);
        assert!(rejected
            .errors
            .values()
            .flatten()
            .any(|message| message.contains("vector_index_compression.dims")));

        settings.search.vector_index_compression.dims = 512;
        assert!(repo.validate_settings(&settings).valid);

        settings.search.vector_index_compression.dims = MAX_VECTOR_COMPRESSION_DIMS + 1;
        assert!(!repo.validate_settings(&settings).valid);
    }
}
