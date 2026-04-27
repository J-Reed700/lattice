use crate::features::custom_model::repository::{
    CustomModelRepository, CustomModelRepositoryTrait,
};
use crate::features::custom_model::services::FileValidationService;
use crate::shared::error::AppError;
use std::sync::Arc;
use uuid::Uuid;

/// Use case for deleting a custom model
pub struct DeleteUseCase {
    repository: Arc<dyn CustomModelRepositoryTrait>,
    file_validator: Arc<FileValidationService>,
}

impl DeleteUseCase {
    /// Create a new instance
    pub fn new(
        repository: Arc<dyn CustomModelRepositoryTrait>,
        file_validator: Arc<FileValidationService>,
    ) -> Self {
        Self {
            repository,
            file_validator,
        }
    }

    /// Execute the use case: Delete model
    ///
    /// # Arguments
    /// * `model_id` - ID of the model to delete (model_id string, not UUID)
    /// * `delete_file` - Whether to also delete the file from disk
    pub async fn execute(&self, model_id: String, delete_file: bool) -> Result<(), AppError> {
        // Find model by model_id (not UUID)
        let model = self
            .repository
            .find_by_model_id(&model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model {} not found", model_id)))?;

        // Delete file if requested
        if delete_file {
            self.file_validator.delete_model_file(&model).await?;
        }

        // Delete from database using UUID id
        let uuid_id = Uuid::parse_str(model.id())
            .map_err(|e| AppError::InvalidData(format!("Invalid UUID: {}", e)))?;
        self.repository.delete(&uuid_id).await?;

        Ok(())
    }
}
