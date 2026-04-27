//! Reset Settings Use Case

use crate::features::settings::dto::{ResetSettingsRequestDto, SettingsCategory, SettingsDto};
use crate::application::ports::SettingsRepositoryPort;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for resetting application settings to defaults.
///
/// # Responsibilities
///
/// - Reset all settings or a specific category to default values
/// - Preserve settings not being reset
pub struct ResetSettingsUseCase {
    repository: Arc<dyn SettingsRepositoryPort>,
}

impl ResetSettingsUseCase {
    /// Create a new use case instance.
    ///
    /// # Arguments
    ///
    /// * `repository` - Settings repository port implementation
    pub fn new(repository: Arc<dyn SettingsRepositoryPort>) -> Self {
        Self { repository }
    }

    /// Execute the use case to reset settings.
    ///
    /// # Arguments
    ///
    /// * `request` - Reset request with optional category
    ///
    /// # Returns
    ///
    /// Settings structure after reset
    ///
    /// # Errors
    ///
    /// Returns error if reset cannot be saved
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let use_case = ResetSettingsUseCase::new(repository);
    ///
    /// // Reset only search settings
    /// let request = ResetSettingsRequestDto {
    ///     category: Some(SettingsCategory::Search),
    /// };
    /// let settings = use_case.execute(request).await?;
    ///
    /// // Reset all settings
    /// let request = ResetSettingsRequestDto { category: None };
    /// let settings = use_case.execute(request).await?;
    /// ```
    pub async fn execute(&self, request: ResetSettingsRequestDto) -> Result<SettingsDto> {
        self.repository.reset(request.category).await
    }

    /// Reset a specific settings category.
    ///
    /// # Arguments
    ///
    /// * `category` - Category to reset
    ///
    /// # Returns
    ///
    /// Settings structure after reset
    pub async fn reset_category(&self, category: SettingsCategory) -> Result<SettingsDto> {
        let request = ResetSettingsRequestDto {
            category: Some(category),
        };
        self.execute(request).await
    }

    /// Reset all settings to defaults.
    ///
    /// # Returns
    ///
    /// Settings structure with all defaults
    pub async fn reset_all(&self) -> Result<SettingsDto> {
        let request = ResetSettingsRequestDto { category: None };
        self.execute(request).await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::settings::dto::UpdateSettingsRequestDto;
    use crate::application::ports::MockSettingsRepository;
    use crate::features::settings::use_cases::update::UpdateSettingsUseCase;
    use serde_json::json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_reset_search_category() {
        let repository = Arc::new(MockSettingsRepository::new());

        // First, modify search settings
        let update_use_case = UpdateSettingsUseCase::new(repository.clone());
        let mut updates = HashMap::new();
        updates.insert("maxResults".to_string(), json!(50));
        update_use_case
            .update_category(SettingsCategory::Search, updates)
            .await
            .unwrap();

        // Verify settings were changed
        let settings = repository.get_all().await.unwrap();
        assert_eq!(settings.search.max_results, 50);

        // Reset search category
        let reset_use_case = ResetSettingsUseCase::new(repository.clone());
        let request = ResetSettingsRequestDto {
            category: Some(SettingsCategory::Search),
        };
        let result = reset_use_case.execute(request).await.unwrap();

        // Verify reset to default
        assert_eq!(result.search.max_results, 10);
    }

    #[tokio::test]
    async fn test_reset_indexing_category() {
        let repository = Arc::new(MockSettingsRepository::new());

        // Modify indexing settings
        let update_use_case = UpdateSettingsUseCase::new(repository.clone());
        let mut updates = HashMap::new();
        updates.insert("chunkSize".to_string(), json!(2048));
        update_use_case
            .update_category(SettingsCategory::Indexing, updates)
            .await
            .unwrap();

        // Reset indexing category
        let reset_use_case = ResetSettingsUseCase::new(repository.clone());
        let result = reset_use_case
            .reset_category(SettingsCategory::Indexing)
            .await
            .unwrap();

        // Verify reset to default
        assert_eq!(result.indexing.chunk_size, 800);
    }

    #[tokio::test]
    async fn test_reset_all() {
        let repository = Arc::new(MockSettingsRepository::new());

        // Modify multiple settings
        let update_use_case = UpdateSettingsUseCase::new(repository.clone());

        let mut search_updates = HashMap::new();
        search_updates.insert("maxResults".to_string(), json!(100));
        update_use_case
            .update_category(SettingsCategory::Search, search_updates)
            .await
            .unwrap();

        let mut indexing_updates = HashMap::new();
        indexing_updates.insert("chunkSize".to_string(), json!(2048));
        update_use_case
            .update_category(SettingsCategory::Indexing, indexing_updates)
            .await
            .unwrap();

        // Reset all settings
        let reset_use_case = ResetSettingsUseCase::new(repository.clone());
        let result = reset_use_case.reset_all().await.unwrap();

        // Verify all reset to defaults
        assert_eq!(result.search.max_results, 10);
        assert_eq!(result.indexing.chunk_size, 800);
        assert_eq!(result.llm.temperature, 0.7);
    }

    #[tokio::test]
    async fn test_reset_category_preserves_others() {
        let repository = Arc::new(MockSettingsRepository::new());

        // Modify multiple categories
        let update_use_case = UpdateSettingsUseCase::new(repository.clone());

        let mut search_updates = HashMap::new();
        search_updates.insert("maxResults".to_string(), json!(100));
        update_use_case
            .update_category(SettingsCategory::Search, search_updates)
            .await
            .unwrap();

        let mut indexing_updates = HashMap::new();
        indexing_updates.insert("chunkSize".to_string(), json!(2048));
        update_use_case
            .update_category(SettingsCategory::Indexing, indexing_updates)
            .await
            .unwrap();

        // Reset only search category
        let reset_use_case = ResetSettingsUseCase::new(repository.clone());
        let result = reset_use_case
            .reset_category(SettingsCategory::Search)
            .await
            .unwrap();

        // Verify search is reset but indexing is preserved
        assert_eq!(result.search.max_results, 10);
        assert_eq!(result.indexing.chunk_size, 2048); // Still modified
    }
}
