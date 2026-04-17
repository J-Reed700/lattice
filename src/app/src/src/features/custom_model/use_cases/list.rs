use crate::features::custom_model::domain::{CustomModel, TaskType, ValidationStatus};
use crate::features::custom_model::repository::{
    CustomModelRepository, CustomModelRepositoryTrait,
};
use crate::shared::error::AppError;
use std::sync::Arc;

/// Use case for listing custom models
pub struct ListUseCase {
    repository: Arc<dyn CustomModelRepositoryTrait>,
}

impl ListUseCase {
    /// Create a new instance
    pub fn new(repository: Arc<dyn CustomModelRepositoryTrait>) -> Self {
        Self { repository }
    }

    /// Execute the use case: List all models
    pub async fn execute(&self) -> Result<Vec<CustomModel>, AppError> {
        self.repository.find_all().await
    }

    /// List models filtered by task type
    pub async fn execute_by_task_type(
        &self,
        task_type: TaskType,
    ) -> Result<Vec<CustomModel>, AppError> {
        self.repository.find_by_task_type(task_type).await
    }

    /// List models filtered by validation status
    pub async fn execute_by_validation_status(
        &self,
        status: ValidationStatus,
    ) -> Result<Vec<CustomModel>, AppError> {
        self.repository.find_by_validation_status(status).await
    }

    /// List models filtered by both task type and validation status
    pub async fn execute_filtered(
        &self,
        task_type: Option<TaskType>,
        validation_status: Option<ValidationStatus>,
    ) -> Result<Vec<CustomModel>, AppError> {
        match (task_type, validation_status) {
            (Some(task), Some(status)) => {
                // Both filters
                let models = self.repository.find_by_task_type(task).await?;
                Ok(models
                    .into_iter()
                    .filter(|m| m.validation_status() == status)
                    .collect())
            }
            (Some(task), None) => {
                // Task type only
                self.repository.find_by_task_type(task).await
            }
            (None, Some(status)) => {
                // Validation status only
                self.repository.find_by_validation_status(status).await
            }
            (None, None) => {
                // No filters
                self.repository.find_all().await
            }
        }
    }
}
