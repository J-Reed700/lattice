//! # Settings Repository Port
//!
//! Port interface for settings storage and retrieval.
//!
//! This port abstracts settings persistence, allowing different implementations
//! (file-based, database, remote, etc.) while keeping the application layer independent.

use crate::features::settings::dto::{SettingsCategory, SettingsDto};
use crate::shared::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Port interface for settings repository.
///
/// Implementations handle persistence of application settings,
/// supporting read, write, reset, import, and export operations.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` for concurrent access.
///
/// # Example
///
/// ```rust,ignore
/// use crate::application::ports::SettingsRepositoryPort;
///
/// async fn update_search_settings(
///     repo: &impl SettingsRepositoryPort,
/// ) -> Result<()> {
///     let mut settings = repo.get_all().await?;
///     settings.search.max_results = 20;
///     repo.save_all(&settings).await?;
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait SettingsRepositoryPort: Send + Sync {
    /// Get all settings.
    ///
    /// # Returns
    ///
    /// Complete settings structure with all categories
    ///
    /// # Errors
    ///
    /// Returns error if settings cannot be loaded
    async fn get_all(&self) -> Result<SettingsDto>;

    /// Get a specific settings category.
    ///
    /// # Arguments
    ///
    /// * `category` - Category to retrieve
    ///
    /// # Returns
    ///
    /// Settings for the specified category as JSON value
    ///
    /// # Errors
    ///
    /// Returns error if category doesn't exist or cannot be loaded
    async fn get_category(&self, category: SettingsCategory) -> Result<serde_json::Value>;

    /// Save all settings.
    ///
    /// # Arguments
    ///
    /// * `settings` - Complete settings structure to save
    ///
    /// # Errors
    ///
    /// Returns error if settings cannot be persisted
    async fn save_all(&self, settings: &SettingsDto) -> Result<()>;

    /// Update specific settings fields.
    ///
    /// # Arguments
    ///
    /// * `category` - Optional category to update (updates all if None)
    /// * `updates` - Key-value pairs of settings to update
    ///
    /// # Returns
    ///
    /// Updated complete settings structure
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Invalid category
    /// - Invalid field names
    /// - Invalid field values
    /// - Cannot save updated settings
    async fn update(
        &self,
        category: Option<SettingsCategory>,
        updates: HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto>;

    /// Reset settings to defaults.
    ///
    /// # Arguments
    ///
    /// * `category` - Optional category to reset (resets all if None)
    ///
    /// # Returns
    ///
    /// Settings structure after reset
    ///
    /// # Errors
    ///
    /// Returns error if reset cannot be saved
    async fn reset(&self, category: Option<SettingsCategory>) -> Result<SettingsDto>;

    /// Export settings to a file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where settings should be exported (JSON format)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Path is invalid
    /// - Cannot write to path
    /// - Cannot serialize settings
    async fn export(&self, path: &str) -> Result<()>;

    /// Import settings from a file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to settings file (JSON format)
    /// * `merge` - If true, merge with existing settings; if false, replace all
    ///
    /// # Returns
    ///
    /// Settings structure after import
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - File doesn't exist
    /// - Cannot read file
    /// - Invalid JSON format
    /// - Invalid settings structure
    async fn import(&self, path: &str, merge: bool) -> Result<SettingsDto>;

    /// Validate settings structure and values.
    ///
    /// # Arguments
    ///
    /// * `settings` - Settings to validate
    ///
    /// # Returns
    ///
    /// Validation result with errors and warnings
    fn validate(
        &self,
        settings: &SettingsDto,
    ) -> crate::features::settings::dto::ValidationResult;

    /// Check if a file path is valid and accessible.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to validate
    ///
    /// # Returns
    ///
    /// `true` if path exists and is a readable directory
    fn validate_folder_path(&self, path: &str) -> bool;
}

/// Mock implementation for testing.
///
/// # Example
///
/// ```rust,ignore
/// use crate::application::ports::MockSettingsRepository;
///
/// #[tokio::test]
/// async fn test_settings_update() {
///     let repo = MockSettingsRepository::new();
///     let settings = repo.get_all().await.unwrap();
///     assert_eq!(settings.search.max_results, 10);
/// }
/// ```
pub struct MockSettingsRepository {
    settings: tokio::sync::RwLock<SettingsDto>,
}

impl MockSettingsRepository {
    /// Create a new mock repository with default settings.
    pub fn new() -> Self {
        Self {
            settings: tokio::sync::RwLock::new(SettingsDto::default()),
        }
    }

    /// Create with custom initial settings.
    pub fn with_settings(settings: SettingsDto) -> Self {
        Self {
            settings: tokio::sync::RwLock::new(settings),
        }
    }
}

impl Default for MockSettingsRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SettingsRepositoryPort for MockSettingsRepository {
    async fn get_all(&self) -> Result<SettingsDto> {
        Ok(self.settings.read().await.clone())
    }

    async fn get_category(&self, category: SettingsCategory) -> Result<serde_json::Value> {
        let settings = self.settings.read().await;
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
        *self.settings.write().await = settings.clone();
        Ok(())
    }

    async fn update(
        &self,
        category: Option<SettingsCategory>,
        updates: HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto> {
        let mut settings = self.settings.write().await;

        // Apply updates based on category
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

                let mut category_map = category_value
                    .as_object()
                    .ok_or_else(|| {
                        crate::shared::error::AppError::InvalidInput(
                            "Invalid category structure".to_string(),
                        )
                    })?
                    .clone();
                for (key, value) in updates {
                    category_map.insert(key, value);
                }

                // Update the category
                match cat {
                    SettingsCategory::Indexing => {
                        settings.indexing =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Search => {
                        settings.search =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Llm => {
                        settings.llm =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Ui => {
                        settings.ui =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Sync => {
                        settings.sync =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Backup => {
                        settings.backup =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Privacy => {
                        settings.privacy =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                }
            }
            None => {
                // Update all settings (flat structure)
                let mut settings_value = serde_json::to_value(&*settings)?;
                let settings_map = settings_value.as_object_mut().ok_or_else(|| {
                    crate::shared::error::AppError::InvalidInput(
                        "Invalid settings structure".to_string(),
                    )
                })?;

                for (key, value) in updates {
                    settings_map.insert(key, value);
                }

                *settings =
                    serde_json::from_value(serde_json::Value::Object(settings_map.clone()))?;
            }
        }

        Ok(settings.clone())
    }

    async fn reset(&self, category: Option<SettingsCategory>) -> Result<SettingsDto> {
        let mut settings = self.settings.write().await;

        match category {
            Some(cat) => {
                // Reset specific category
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
                // Reset all settings
                *settings = SettingsDto::default();
            }
        }

        Ok(settings.clone())
    }

    async fn export(&self, _path: &str) -> Result<()> {
        // Mock implementation - no-op
        Ok(())
    }

    async fn import(&self, _path: &str, _merge: bool) -> Result<SettingsDto> {
        // Mock implementation - return current settings
        Ok(self.settings.read().await.clone())
    }

    fn validate(
        &self,
        settings: &SettingsDto,
    ) -> crate::features::settings::dto::ValidationResult {
        use crate::features::settings::dto::ValidationResult;

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
        if settings.llm.provider == crate::features::settings::dto::LLMProvider::Ollama {
            if settings.llm.ollama_url.is_empty() {
                result.add_error("llm", "ollama_url cannot be empty".to_string());
            }
            if settings.llm.model.is_empty() {
                result.add_error("llm", "model cannot be empty".to_string());
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

        // Validate UI settings
        if settings.ui.font_size < 8 || settings.ui.font_size > 72 {
            result.add_error("ui", "font_size must be between 8 and 72".to_string());
        }
        if settings.ui.results_per_page == 0 {
            result.add_error("ui", "results_per_page must be greater than 0".to_string());
        }

        // Validate sync settings
        if settings.sync.sync_enabled && settings.sync.sync_url.is_empty() {
            result.add_error(
                "sync",
                "sync_url is required when sync is enabled".to_string(),
            );
        }

        // Validate backup settings
        if settings.backup.auto_backup_enabled && settings.backup.backup_path.is_empty() {
            result.add_error(
                "backup",
                "backup_path is required when auto-backup is enabled".to_string(),
            );
        }

        result
    }

    fn validate_folder_path(&self, path: &str) -> bool {
        use std::path::Path;

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

    #[tokio::test]
    async fn test_mock_repository_get_all() {
        let repo = MockSettingsRepository::new();
        let settings = repo.get_all().await.unwrap();

        assert_eq!(settings.indexing.chunk_size, 800);
        assert_eq!(settings.search.max_results, 10);
    }

    #[tokio::test]
    async fn test_mock_repository_get_category() {
        let repo = MockSettingsRepository::new();
        let category = repo.get_category(SettingsCategory::Search).await.unwrap();

        assert!(category.is_object());
        assert_eq!(category["maxResults"], 10);
    }

    #[tokio::test]
    async fn test_mock_repository_update() {
        let repo = MockSettingsRepository::new();
        let mut updates = HashMap::new();
        updates.insert("maxResults".to_string(), serde_json::json!(20));

        let updated = repo
            .update(Some(SettingsCategory::Search), updates)
            .await
            .unwrap();

        assert_eq!(updated.search.max_results, 20);
    }

    #[tokio::test]
    async fn test_mock_repository_reset() {
        let repo = MockSettingsRepository::new();

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
    async fn test_validation_success() {
        let repo = MockSettingsRepository::new();
        let settings = SettingsDto::default();
        let result = repo.validate(&settings);

        assert!(result.valid);
        assert!(!result.has_errors());
    }

    #[tokio::test]
    async fn test_validation_errors() {
        let repo = MockSettingsRepository::new();
        let mut settings = SettingsDto::default();
        settings.indexing.chunk_size = 0; // Invalid
        settings.search.similarity_threshold = 1.5; // Invalid

        let result = repo.validate(&settings);

        assert!(!result.valid);
        assert!(result.has_errors());
        assert!(result.errors.contains_key("indexing"));
        assert!(result.errors.contains_key("search"));
    }
}
