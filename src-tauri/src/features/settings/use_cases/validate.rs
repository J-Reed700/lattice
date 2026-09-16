//! Validate Settings Use Case

use crate::application::ports::SettingsRepositoryPort;
use crate::features::settings::dto::{SettingsDto, ValidationResult};
use std::sync::Arc;

/// Use case for validating application settings.
///
/// # Responsibilities
///
/// - Validate settings structure and values
/// - Check constraints and business rules
/// - Provide detailed validation feedback
pub struct ValidateSettingsUseCase {
    repository: Arc<dyn SettingsRepositoryPort>,
}

impl ValidateSettingsUseCase {
    /// Create a new use case instance.
    ///
    /// # Arguments
    ///
    /// * `repository` - Settings repository port implementation
    pub fn new(repository: Arc<dyn SettingsRepositoryPort>) -> Self {
        Self { repository }
    }

    /// Execute the use case to validate settings.
    ///
    /// # Arguments
    ///
    /// * `settings` - Settings to validate
    ///
    /// # Returns
    ///
    /// Validation result with errors and warnings
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let use_case = ValidateSettingsUseCase::new(repository);
    /// let settings = SettingsDto::default();
    /// let result = use_case.execute(&settings);
    ///
    /// if !result.valid {
    ///     println!("Validation errors: {:?}", result.errors);
    /// }
    /// if result.has_warnings() {
    ///     println!("Warnings: {:?}", result.warnings);
    /// }
    /// ```
    pub fn execute(&self, settings: &SettingsDto) -> ValidationResult {
        self.repository.validate(settings)
    }

    /// Validate current settings from repository.
    ///
    /// # Returns
    ///
    /// Validation result for the current settings
    ///
    /// # Errors
    ///
    /// Returns error if settings cannot be loaded from repository
    pub async fn validate_current(&self) -> crate::shared::error::Result<ValidationResult> {
        let settings = self.repository.get_all().await?;
        Ok(self.execute(&settings))
    }

    /// Validate a folder path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to validate
    ///
    /// # Returns
    ///
    /// `true` if path exists and is a readable directory
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let use_case = ValidateSettingsUseCase::new(repository);
    /// if use_case.validate_folder_path("/home/user/documents") {
    ///     println!("Path is valid");
    /// }
    /// ```
    pub fn validate_folder_path(&self, path: &str) -> bool {
        self.repository.validate_folder_path(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::MockSettingsRepository;

    #[test]
    fn test_validate_default_settings() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let settings = SettingsDto::default();
        let result = use_case.execute(&settings);

        assert!(result.valid);
        assert!(!result.has_errors());
    }

    #[test]
    fn test_validate_invalid_chunk_size() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let mut settings = SettingsDto::default();
        settings.indexing.chunk_size = 0; // Invalid

        let result = use_case.execute(&settings);

        assert!(!result.valid);
        assert!(result.has_errors());
        assert!(result.errors.contains_key("indexing"));
    }

    #[test]
    fn test_validate_invalid_similarity_threshold() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let mut settings = SettingsDto::default();
        settings.search.similarity_threshold = 1.5; // Invalid (must be 0.0-1.0)

        let result = use_case.execute(&settings);

        assert!(!result.valid);
        assert!(result.has_errors());
        assert!(result.errors.contains_key("search"));
    }

    #[test]
    fn test_validate_invalid_temperature() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let mut settings = SettingsDto::default();
        settings.llm.temperature = 3.0; // Invalid (must be 0.0-2.0)

        let result = use_case.execute(&settings);

        assert!(!result.valid);
        assert!(result.has_errors());
        assert!(result.errors.contains_key("llm"));
    }

    #[test]
    fn test_validate_chunk_overlap_greater_than_size() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let mut settings = SettingsDto::default();
        settings.indexing.chunk_size = 100;
        settings.indexing.chunk_overlap = 150; // Invalid

        let result = use_case.execute(&settings);

        assert!(!result.valid);
        assert!(result.has_errors());
        assert!(result.errors.contains_key("indexing"));
    }

    #[test]
    fn test_validate_warnings() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let mut settings = SettingsDto::default();
        settings.indexing.chunk_size = 15000; // Very large, should trigger warning

        let result = use_case.execute(&settings);

        assert!(result.valid);
        assert!(result.has_warnings());
        assert!(result.warnings.contains_key("indexing"));
    }

    #[test]
    fn test_validate_multiple_errors() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let mut settings = SettingsDto::default();
        settings.indexing.chunk_size = 0; // Invalid
        settings.search.similarity_threshold = 1.5; // Invalid
        settings.llm.temperature = 3.0; // Invalid

        let result = use_case.execute(&settings);

        assert!(!result.valid);
        assert!(result.has_errors());
        assert!(result.errors.contains_key("indexing"));
        assert!(result.errors.contains_key("search"));
        assert!(result.errors.contains_key("llm"));
    }

    #[tokio::test]
    async fn test_validate_current_settings() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let result = use_case.validate_current().await.unwrap();

        assert!(result.valid);
        assert!(!result.has_errors());
    }

    #[test]
    fn test_validate_folder_path() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        // /tmp should exist on most systems
        let is_valid = use_case.validate_folder_path("/tmp");
        assert!(is_valid);

        // Nonexistent path
        let is_valid = use_case.validate_folder_path("/nonexistent/path/12345");
        assert!(!is_valid);
    }

    #[test]
    fn test_validate_sync_settings() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let mut settings = SettingsDto::default();
        settings.sync.sync_enabled = true;
        settings.sync.sync_url = String::new(); // Invalid when sync is enabled

        let result = use_case.execute(&settings);

        assert!(!result.valid);
        assert!(result.has_errors());
        assert!(result.errors.contains_key("sync"));
    }

    #[test]
    fn test_validate_backup_settings() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ValidateSettingsUseCase::new(repository);

        let mut settings = SettingsDto::default();
        settings.backup.auto_backup_enabled = true;
        settings.backup.backup_path = String::new();

        let result = use_case.execute(&settings);

        assert!(result.valid);
        assert!(!result.has_errors());

        settings.backup.backup_path = "/tmp/custom-backups".to_string();
        let result = use_case.execute(&settings);
        assert!(!result.valid);
        assert!(result.errors.contains_key("backup"));
    }
}
