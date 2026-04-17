use crate::features::custom_model::domain::{
    CustomModel, ModelArchitecture, ModelId, ModelName, ModelSource, TaskType,
};
use crate::features::custom_model::repository::{
    CustomModelRepository, CustomModelRepositoryTrait,
};
use crate::features::custom_model::services::{
    ArchitectureInferenceService, UrlValidationService,
};
use crate::shared::error::AppError;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Use case for adding a custom model from a URL
pub struct AddFromUrlUseCase {
    repository: Arc<dyn CustomModelRepositoryTrait>,
    url_validator: Arc<UrlValidationService>,
    architecture_inferrer: Arc<ArchitectureInferenceService>,
    pool: SqlitePool,
}

impl AddFromUrlUseCase {
    /// Create a new instance
    pub fn new(
        repository: Arc<dyn CustomModelRepositoryTrait>,
        url_validator: Arc<UrlValidationService>,
        architecture_inferrer: Arc<ArchitectureInferenceService>,
        pool: SqlitePool,
    ) -> Self {
        Self {
            repository,
            url_validator,
            architecture_inferrer,
            pool,
        }
    }

    /// Execute the use case: Add model from URL
    ///
    /// Steps:
    /// 1. Validate URL (SSRF prevention)
    /// 2. Create value objects (ModelName, ModelId, ModelSource)
    /// 3. Create CustomModel in "Pending" state
    /// 4. Save to repository
    /// 5. Spawn async validation task
    /// 6. Return model ID
    pub async fn execute(
        &self,
        url: String,
        name: String,
        model_id: String,
        task_type: TaskType,
        architecture: Option<ModelArchitecture>,
    ) -> Result<String, AppError> {
        // Step 1: Validate URL (SSRF prevention)
        let validated_url = match self.url_validator.validate_url_pre_save(&url).await {
            Ok(url) => url,
            Err(e) => {
                return Err(e);
            }
        };

        // Step 2: Create value objects
        let model_name = ModelName::new(name.clone())
            .map_err(|e| AppError::InvalidInput(format!("Invalid model name: {}", e)))?;

        let model_identifier = ModelId::new(model_id.clone())
            .map_err(|e| AppError::InvalidInput(format!("Invalid model ID: {}", e)))?;

        let source = ModelSource::Url(validated_url.to_string());

        let inferred_architecture = self.architecture_inferrer.infer_from_url(&validated_url);
        if let Some(requested_architecture) = architecture {
            if requested_architecture != inferred_architecture {
                return Err(AppError::InvalidInput(format!(
                    "Provided architecture '{}' does not match inferred architecture '{}'",
                    format!("{:?}", requested_architecture),
                    format!("{:?}", inferred_architecture)
                )));
            }
        }

        let resolved_architecture = architecture.unwrap_or(inferred_architecture);
        let extension = match resolved_architecture {
            ModelArchitecture::Gguf => "gguf",
            ModelArchitecture::Onnx => "onnx",
            ModelArchitecture::SafeTensors => "safetensors",
            ModelArchitecture::PyTorch => "pt",
        };

        // Step 3: Create CustomModel in "Pending" state
        // Note: We use placeholder file path/size since download hasn't happened yet
        // These will be updated after successful download
        let placeholder_path = std::env::temp_dir()
            .join("models")
            .join(format!("{}.{}", model_id, extension));

        // IMPORTANT: Use 1 byte as placeholder to satisfy database constraint
        // CHECK(file_size_bytes > 0 AND file_size_bytes <= 5368709120)
        // This will be updated to the actual file size after download completes
        let placeholder_size = 1i64;

        let model = CustomModel::new(
            model_name,
            model_identifier,
            source,
            placeholder_path,
            placeholder_size,
            task_type,
            None, // metadata will be populated after download
        )
        .map_err(|e| AppError::InvalidInput(format!("Failed to create model: {}", e)))?;

        // Step 4: Save to repository
        self.repository.save(&model).await?;

        // Step 5: Spawn async validation task with timeout
        let pool_clone = self.pool.clone();
        let model_id_clone = model.id().to_string();
        let url_clone = url.clone();
        let url_validator_clone = self.url_validator.clone();
        let repository_clone = self.repository.clone();

        tokio::spawn(async move {
            // Apply 30-second timeout to prevent indefinite hangs
            let validation_future = validate_url_async(
                pool_clone,
                model_id_clone.clone(),
                url_clone,
                url_validator_clone,
                repository_clone,
            );

            match tokio::time::timeout(std::time::Duration::from_secs(30), validation_future).await
            {
                Ok(Ok(_)) => {
                    tracing::info!(
                        "Async URL validation succeeded for model {}",
                        model_id_clone
                    );
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        "Async URL validation failed for model {}: {}",
                        model_id_clone,
                        e
                    );
                }
                Err(_) => {
                    tracing::error!(
                        "Async URL validation timed out for model {} after 30 seconds",
                        model_id_clone
                    );
                }
            }
        });

        // Step 6: Return model ID
        Ok(model.id().to_string())
    }
}

/// Background async validation task
async fn validate_url_async(
    _pool: SqlitePool,
    model_id: String,
    url: String,
    url_validator: Arc<UrlValidationService>,
    repository: Arc<dyn CustomModelRepositoryTrait>,
) -> Result<(), AppError> {
    // Parse URL
    let parsed_url =
        url::Url::parse(&url).map_err(|e| AppError::InvalidInput(format!("Invalid URL: {}", e)))?;

    // Parse model ID as UUID
    let model_uuid = Uuid::parse_str(&model_id)
        .map_err(|e| AppError::InvalidInput(format!("Invalid UUID: {}", e)))?;

    // Deep validation
    let validation_result = url_validator.validate_url_deep(&parsed_url).await;

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
