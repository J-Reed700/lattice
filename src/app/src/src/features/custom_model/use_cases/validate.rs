use crate::features::custom_model::domain::{CustomModel, ModelSource, ValidationStatus};
use crate::features::custom_model::repository::{
    CustomModelRepository, CustomModelRepositoryTrait,
};
use crate::features::custom_model::services::{FileValidationService, UrlValidationService};
use crate::shared::error::AppError;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// Use case for validating a custom model
pub struct ValidateUseCase {
    repository: Arc<dyn CustomModelRepositoryTrait>,
    url_validator: Arc<UrlValidationService>,
    file_validator: Arc<FileValidationService>,
}

impl ValidateUseCase {
    /// Create a new instance
    pub fn new(
        repository: Arc<dyn CustomModelRepositoryTrait>,
        url_validator: Arc<UrlValidationService>,
        file_validator: Arc<FileValidationService>,
    ) -> Self {
        Self {
            repository,
            url_validator,
            file_validator,
        }
    }

    /// Execute the use case: Validate a model
    ///
    /// Validates either URL or file depending on model source
    pub async fn execute(&self, model_id: String) -> Result<ValidationStatus, AppError> {
        // Parse model ID as UUID
        let model_uuid = Uuid::parse_str(&model_id)
            .map_err(|e| AppError::InvalidInput(format!("Invalid UUID: {}", e)))?;

        // Find model
        let mut model = self
            .repository
            .find_by_id(&model_uuid)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model {} not found", model_id)))?;

        // Mark as validating before starting validation
        model.mark_validating();

        // Determine validation type based on model source
        let validation_result = match model.source() {
            ModelSource::Url(url) => self.validate_url(url).await,
            ModelSource::LocalFile(path) => self.validate_file(path).await,
        };

        // Update model validation status using domain methods
        match validation_result {
            Ok(_) => {
                model.mark_valid();
            }
            Err(e) => {
                model.mark_invalid(e.to_string());
            }
        }

        // Save updated model
        let status = model.validation_status();
        self.repository.save(&model).await?;

        Ok(status)
    }

    /// Validate URL-based model
    async fn validate_url(&self, url: &str) -> Result<(), AppError> {
        let parsed_url = url::Url::parse(url)
            .map_err(|e| AppError::InvalidInput(format!("Invalid URL: {}", e)))?;

        self.url_validator.validate_url_deep(&parsed_url).await
    }

    /// Validate file-based model
    async fn validate_file(&self, file_path: &Path) -> Result<(), AppError> {
        self.file_validator.validate_file_deep(file_path).await
    }
}
