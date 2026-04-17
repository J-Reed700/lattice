use crate::features::custom_model::domain::{
    CustomModel, ModelArchitecture, ModelId, ModelName, ModelSource, TaskType,
};
use crate::features::custom_model::repository::{
    CustomModelRepository, CustomModelRepositoryTrait,
};
use crate::features::custom_model::services::{
    ArchitectureInferenceService, FileValidationService,
};
use crate::shared::error::AppError;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Use case for adding a custom model from a file
pub struct AddFromFileUseCase {
    repository: Arc<dyn CustomModelRepositoryTrait>,
    file_validator: Arc<FileValidationService>,
    architecture_inferrer: Arc<ArchitectureInferenceService>,
    pool: SqlitePool,
}

impl AddFromFileUseCase {
    /// Create a new instance
    pub fn new(
        repository: Arc<dyn CustomModelRepositoryTrait>,
        file_validator: Arc<FileValidationService>,
        architecture_inferrer: Arc<ArchitectureInferenceService>,
        pool: SqlitePool,
    ) -> Self {
        Self {
            repository,
            file_validator,
            architecture_inferrer,
            pool,
        }
    }

    /// Execute the use case: Add model from file
    ///
    /// Steps:
    /// 1. Validate file (size, extension, path safety)
    /// 2. Get file size for validation
    /// 3. Create value objects (ModelName, ModelId, ModelSource)
    /// 4. Create CustomModel in "Pending" state
    /// 5. Copy to models directory (future enhancement)
    /// 6. Save to repository
    /// 7. Spawn async validation task
    /// 8. Return model ID
    pub async fn execute(
        &self,
        file_path: String,
        name: String,
        model_id: String,
        task_type: TaskType,
        architecture: Option<ModelArchitecture>,
    ) -> Result<String, AppError> {
        // Step 1: Validate file
        let validated_path = match self.file_validator.validate_file(&file_path).await {
            Ok(path) => path,
            Err(e) => {
                return Err(e);
            }
        };

        // Step 2: Get file size
        let file_metadata = tokio::fs::metadata(validated_path.as_path())
            .await
            .map_err(|e| AppError::InvalidInput(format!("Failed to read file metadata: {}", e)))?;
        let file_size = file_metadata.len() as i64;

        let inferred_architecture = self
            .architecture_inferrer
            .infer_from_file(validated_path.as_path());
        if let Some(requested_architecture) = architecture {
            if requested_architecture != inferred_architecture {
                return Err(AppError::InvalidInput(format!(
                    "Provided architecture '{}' does not match inferred architecture '{}'",
                    format!("{:?}", requested_architecture),
                    format!("{:?}", inferred_architecture)
                )));
            }
        }

        // Step 3: Create value objects
        let model_name = ModelName::new(name.clone())
            .map_err(|e| AppError::InvalidInput(format!("Invalid model name: {}", e)))?;

        let model_identifier = ModelId::new(model_id.clone())
            .map_err(|e| AppError::InvalidInput(format!("Invalid model ID: {}", e)))?;

        let source = ModelSource::LocalFile(validated_path.as_path().to_path_buf());

        // Step 4: Create CustomModel in "Pending" state
        let model = CustomModel::new(
            model_name,
            model_identifier,
            source,
            validated_path.as_path().to_path_buf(),
            file_size,
            task_type,
            None, // metadata will be populated after validation
        )
        .map_err(|e| AppError::InvalidInput(format!("Failed to create model: {}", e)))?;

        // Step 5: Copy file to models directory
        let copied_path = self
            .file_validator
            .copy_to_models_dir(&validated_path, &model)
            .await?;
        let copied_size = tokio::fs::metadata(&copied_path)
            .await
            .map_err(|e| AppError::InvalidInput(format!("Failed to read copied file: {}", e)))?
            .len() as i64;

        let updated_model = CustomModel::from_db(
            model.id().to_string(),
            model.name().clone(),
            model.model_id().clone(),
            ModelSource::LocalFile(copied_path.clone()),
            copied_path.clone(),
            copied_size,
            model.architecture(),
            model.task_type(),
            model.validation_status(),
            model.validation_error().map(|err| err.to_string()),
            model.metadata().cloned(),
            *model.created_at(),
            *model.updated_at(),
            model.last_validated_at().cloned(),
        )
        .map_err(|e| AppError::InvalidInput(format!("Failed to update model path: {}", e)))?;

        // Step 6: Save to repository
        self.repository.save(&updated_model).await?;

        // Step 7: Spawn async validation task with timeout
        let pool_clone = self.pool.clone();
        let model_id_clone = model.id().to_string();
        let validated_path_clone = copied_path;
        let file_validator_clone = self.file_validator.clone();
        let repository_clone = self.repository.clone();

        tokio::spawn(async move {
            // Apply 30-second timeout to prevent indefinite hangs
            let validation_future = validate_file_async(
                pool_clone,
                model_id_clone.clone(),
                validated_path_clone,
                file_validator_clone,
                repository_clone,
            );

            match tokio::time::timeout(std::time::Duration::from_secs(30), validation_future).await
            {
                Ok(Ok(_)) => {
                    tracing::info!(
                        "Async file validation succeeded for model {}",
                        model_id_clone
                    );
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        "Async file validation failed for model {}: {}",
                        model_id_clone,
                        e
                    );
                }
                Err(_) => {
                    tracing::error!(
                        "Async file validation timed out for model {} after 30 seconds",
                        model_id_clone
                    );
                }
            }
        });

        // Step 8: Return model ID
        Ok(model.id().to_string())
    }
}

/// Background async validation task
async fn validate_file_async(
    _pool: SqlitePool,
    model_id: String,
    file_path: PathBuf,
    file_validator: Arc<FileValidationService>,
    repository: Arc<dyn CustomModelRepositoryTrait>,
) -> Result<(), AppError> {
    // Parse model ID as UUID
    let model_uuid = Uuid::parse_str(&model_id)
        .map_err(|e| AppError::InvalidInput(format!("Invalid UUID: {}", e)))?;

    // Deep validation
    let validation_result = file_validator.validate_file_deep(&file_path).await;

    // Update model validation status
    let mut model = repository
        .find_by_id(&model_uuid)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Model {} not found", model_id)))?;

    match validation_result {
        Ok(_) => {
            model.mark_valid();
        }
        Err(e) => {
            model.mark_invalid(e.to_string());
        }
    }

    repository.save(&model).await?;

    Ok(())
}
