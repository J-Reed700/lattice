//! Get Settings Use Case

use crate::application::dtos::settings::{SettingsCategory, SettingsDto};
use crate::application::ports::SettingsRepositoryPort;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for retrieving application settings.
///
/// # Responsibilities
///
/// - Retrieve all settings or a specific category
/// - Provide read-only access to settings
pub struct GetSettingsUseCase {
    repository: Arc<dyn SettingsRepositoryPort>,
}

impl GetSettingsUseCase {
    /// Create a new use case instance.
    ///
    /// # Arguments
    ///
    /// * `repository` - Settings repository port implementation
    pub fn new(repository: Arc<dyn SettingsRepositoryPort>) -> Self {
        Self { repository }
    }

    /// Get all application settings.
    ///
    /// # Returns
    ///
    /// Complete settings structure with all categories
    ///
    /// # Errors
    ///
    /// Returns error if settings cannot be loaded from repository
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let use_case = GetSettingsUseCase::new(repository);
    /// let settings = use_case.execute().await?;
    /// println!("Chunk size: {}", settings.indexing.chunk_size);
    /// ```
    pub async fn execute(&self) -> Result<SettingsDto> {
        self.repository.get_all().await
    }

    /// Get a specific settings category.
    ///
    /// # Arguments
    ///
    /// * `category` - Category to retrieve (indexing, search, llm, ui, sync, backup)
    ///
    /// # Returns
    ///
    /// Settings for the specified category as JSON value
    ///
    /// # Errors
    ///
    /// Returns error if category doesn't exist or cannot be loaded
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let use_case = GetSettingsUseCase::new(repository);
    /// let search_settings = use_case.get_category(SettingsCategory::Search).await?;
    /// ```
    pub async fn get_category(&self, category: SettingsCategory) -> Result<serde_json::Value> {
        self.repository.get_category(category).await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::MockSettingsRepository;

    #[tokio::test]
    async fn test_get_all_settings() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = GetSettingsUseCase::new(repository);

        let result = use_case.execute().await.unwrap();

        assert_eq!(result.indexing.chunk_size, 800);
        assert_eq!(result.search.max_results, 10);
        assert_eq!(result.llm.model, "llama3.2:latest");
    }

    #[tokio::test]

    async fn test_get_category() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = GetSettingsUseCase::new(repository);

        let result = use_case
            .get_category(SettingsCategory::Search)
            .await
            .unwrap();

        assert!(result.is_object());
        assert_eq!(result["maxResults"], 10);
        // Use approximate comparison for floating-point due to JSON serialization precision loss
        let threshold = result["similarityThreshold"].as_f64().unwrap();
        assert!(
            (threshold - 0.7).abs() < 0.01,
            "Expected ~0.7, got {}",
            threshold
        );
    }

    #[tokio::test]
    async fn test_get_indexing_category() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = GetSettingsUseCase::new(repository);

        let result = use_case
            .get_category(SettingsCategory::Indexing)
            .await
            .unwrap();

        assert!(result.is_object());
        assert_eq!(result["chunkSize"], 512);
        assert_eq!(result["chunkOverlap"], 50);
    }

    #[tokio::test]

    async fn test_get_llm_category() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = GetSettingsUseCase::new(repository);

        let result = use_case.get_category(SettingsCategory::Llm).await.unwrap();

        assert!(result.is_object());
        assert_eq!(result["model"], "llama3.2:latest");
        // Use approximate comparison for floating-point due to JSON serialization precision loss
        let temperature = result["temperature"].as_f64().unwrap();
        assert!(
            (temperature - 0.7).abs() < 0.01,
            "Expected ~0.7, got {}",
            temperature
        );
    }
}
