//! Update Settings Use Case

use crate::application::dtos::settings::{SettingsCategory, SettingsDto, UpdateSettingsRequestDto};
use crate::application::ports::SettingsRepositoryPort;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for updating application settings.
///
/// # Responsibilities
///
/// - Update specific settings fields
/// - Support category-specific or global updates
/// - Validate updates before applying
pub struct UpdateSettingsUseCase {
    repository: Arc<dyn SettingsRepositoryPort>,
}

impl UpdateSettingsUseCase {
    /// Create a new use case instance.
    ///
    /// # Arguments
    ///
    /// * `repository` - Settings repository port implementation
    pub fn new(repository: Arc<dyn SettingsRepositoryPort>) -> Self {
        Self { repository }
    }

    /// Execute the use case to update settings.
    ///
    /// # Arguments
    ///
    /// * `request` - Update request with category and field updates
    ///
    /// # Returns
    ///
    /// Updated complete settings structure
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Invalid category specified
    /// - Invalid field names in updates
    /// - Invalid field values
    /// - Validation fails
    /// - Cannot save updated settings
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let use_case = UpdateSettingsUseCase::new(repository);
    /// let mut updates = HashMap::new();
    /// updates.insert("maxResults".to_string(), json!(20));
    ///
    /// let request = UpdateSettingsRequestDto {
    ///     category: Some(SettingsCategory::Search),
    ///     updates,
    /// };
    ///
    /// let settings = use_case.execute(request).await?;
    /// assert_eq!(settings.search.max_results, 20);
    /// ```
    pub async fn execute(&self, request: UpdateSettingsRequestDto) -> Result<SettingsDto> {
        // Apply updates
        let updated_settings = self
            .repository
            .update(request.category, request.updates)
            .await?;

        // Validate updated settings
        let validation = self.repository.validate(&updated_settings);
        if !validation.valid {
            return Err(crate::error::AppError::InvalidInput(format!(
                "Settings validation failed: {:?}",
                validation.errors
            )));
        }

        // Save validated settings
        self.repository.save_all(&updated_settings).await?;

        Ok(updated_settings)
    }

    /// Update a specific settings category.
    ///
    /// # Arguments
    ///
    /// * `category` - Category to update
    /// * `updates` - Key-value pairs of field updates
    ///
    /// # Returns
    ///
    /// Updated complete settings structure
    pub async fn update_category(
        &self,
        category: SettingsCategory,
        updates: std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto> {
        let request = UpdateSettingsRequestDto {
            category: Some(category),
            updates,
        };
        self.execute(request).await
    }

    /// Update global settings (across all categories).
    ///
    /// # Arguments
    ///
    /// * `updates` - Key-value pairs of field updates
    ///
    /// # Returns
    ///
    /// Updated complete settings structure
    pub async fn update_global(
        &self,
        updates: std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto> {
        let request = UpdateSettingsRequestDto {
            category: None,
            updates,
        };
        self.execute(request).await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::MockSettingsRepository;
    use serde_json::json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_update_search_category() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("maxResults".to_string(), json!(20));
        updates.insert("similarityThreshold".to_string(), json!(0.8));

        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Search),
            updates,
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.search.max_results, 20);
        assert_eq!(result.search.similarity_threshold, 0.8);
    }

    #[tokio::test]
    async fn test_update_indexing_category() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("chunkSize".to_string(), json!(1024));
        updates.insert("chunkOverlap".to_string(), json!(100));

        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Indexing),
            updates,
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.indexing.chunk_size, 1024);
        assert_eq!(result.indexing.chunk_overlap, 100);
    }

    #[tokio::test]
    async fn test_update_category_helper() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        updates.insert("temperature".to_string(), json!(0.9));
        updates.insert("maxTokens".to_string(), json!(4096));

        let result = use_case
            .update_category(SettingsCategory::Llm, updates)
            .await
            .unwrap();

        assert_eq!(result.llm.temperature, 0.9);
        assert_eq!(result.llm.max_tokens, 4096);
    }

    #[tokio::test]
    async fn test_validation_failure() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        // Invalid: chunk_size must be > 0
        updates.insert("chunkSize".to_string(), json!(0));

        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Indexing),
            updates,
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validation_out_of_range() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = UpdateSettingsUseCase::new(repository);

        let mut updates = HashMap::new();
        // Invalid: similarity_threshold must be between 0.0 and 1.0
        updates.insert("similarityThreshold".to_string(), json!(1.5));

        let request = UpdateSettingsRequestDto {
            category: Some(SettingsCategory::Search),
            updates,
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
    }
}
