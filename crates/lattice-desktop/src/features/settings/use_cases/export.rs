//! Export Settings Use Case

use crate::features::settings::dto::{ExportSettingsRequestDto, ExportSettingsResponseDto};
use lattice_core::application::ports::SettingsRepositoryPort;
use lattice_core::shared::error::Result;
use std::path::PathBuf;
use std::sync::Arc;

/// Use case for exporting application settings to a file.
///
/// # Responsibilities
///
/// - Export current settings to JSON file
/// - Validate export path
/// - Handle file I/O errors
pub struct ExportSettingsUseCase {
    repository: Arc<dyn SettingsRepositoryPort>,
}

impl ExportSettingsUseCase {
    /// Create a new use case instance.
    pub fn new(repository: Arc<dyn SettingsRepositoryPort>) -> Self {
        Self { repository }
    }

    /// Execute the use case to export settings.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Path is invalid or not writable
    /// - Cannot serialize settings to JSON
    /// - File write operation fails
    pub async fn execute(
        &self,
        request: ExportSettingsRequestDto,
    ) -> Result<ExportSettingsResponseDto> {
        // Validate path before attempting export
        self.validate_export_path(&request.path)?;

        // Perform export through repository
        self.repository.export(&request.path).await?;

        Ok(ExportSettingsResponseDto {
            path: PathBuf::from(&request.path),
            status: "Settings exported successfully".to_string(),
        })
    }

    /// Validate the export path.
    ///
    /// # Errors
    ///
    /// Returns error if path is empty or parent directory doesn't exist
    fn validate_export_path(&self, path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(lattice_core::shared::error::AppError::InvalidInput(
                "Export path cannot be empty".to_string(),
            ));
        }

        let path_buf = PathBuf::from(path);

        if let Some(parent) = path_buf.parent() {
            // repository-barrier-allow: export targets a user-selected filesystem resource.
            if !parent.exists() {
                return Err(lattice_core::shared::error::AppError::InvalidInput(
                    format!("Parent directory does not exist: {}", parent.display()),
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::ports::MockSettingsRepository;

    fn temp_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(file_name)
    }

    #[tokio::test]
    async fn test_export_settings() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ExportSettingsUseCase::new(repository);

        let request = ExportSettingsRequestDto {
            path: temp_path("test-settings.json")
                .to_string_lossy()
                .to_string(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.path, temp_path("test-settings.json"));
        assert!(result.status.contains("success"));
    }

    #[tokio::test]
    async fn test_export_validation_empty_path() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ExportSettingsUseCase::new(repository);

        let request = ExportSettingsRequestDto {
            path: String::new(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_export_path_empty() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ExportSettingsUseCase::new(repository);

        let result = use_case.validate_export_path("");

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_export_path_valid() {
        let repository = Arc::new(MockSettingsRepository::new());
        let use_case = ExportSettingsUseCase::new(repository);

        let result = use_case.validate_export_path(&temp_path("settings.json").to_string_lossy());

        assert!(result.is_ok());
    }
}
