use crate::application::use_cases::custom_model::{
    AddFromFileUseCase, AddFromUrlUseCase, DeleteUseCase, ListUseCase, ValidateUseCase,
};
use crate::domain::custom_model::{CustomModel, ModelArchitecture, TaskType, ValidationStatus};
use crate::infrastructure::audit::{get_audit_logger, AuditAction};
use crate::infrastructure::persistence::repositories::{
    CustomModelRepository, CustomModelRepositoryTrait,
};
use crate::infrastructure::services::custom_model::{
    ArchitectureInferenceService, FileValidationService, UrlValidationService,
};
use crate::interfaces::di::Container;
use crate::{audit_failure, audit_success};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State};
use tracing::{error, info};

/// DTO for adding a custom model from URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddModelFromUrlRequest {
    pub url: String,
    pub name: String,
    pub model_id: String,
    pub task_type: String,
    pub architecture: Option<String>,
}

/// DTO for adding a custom model from file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddModelFromFileRequest {
    pub file_path: String,
    pub name: String,
    pub model_id: String,
    pub task_type: String,
    pub architecture: Option<String>,
}

/// DTO for custom model response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomModelResponse {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub architecture: String,
    pub url: Option<String>,
    pub file_path: Option<String>,
    pub validation_status: String,
    pub validation_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<CustomModel> for CustomModelResponse {
    fn from(model: CustomModel) -> Self {
        // Extract URL and file path from source
        let (url, file_path) = match model.source() {
            crate::domain::custom_model::ModelSource::Url(u) => (Some(u.clone()), None),
            crate::domain::custom_model::ModelSource::LocalFile(p) => {
                (None, Some(p.to_string_lossy().to_string()))
            }
        };

        Self {
            id: model.id().to_string(),
            name: model.name().as_str().to_string(),
            task_type: format!("{:?}", model.task_type()),
            architecture: format!("{:?}", model.architecture()),
            url,
            file_path,
            validation_status: format!("{:?}", model.validation_status()),
            validation_error: model.validation_error().map(|s| s.to_string()),
            created_at: model.created_at().to_rfc3339(),
            updated_at: model.updated_at().to_rfc3339(),
        }
    }
}

/// Add a custom model from URL
///
/// Validates URL (SSRF prevention), creates model in "Pending" state,
/// and spawns background validation task.
pub async fn add_custom_model_from_url(
    container: State<'_, Container>,
    request: AddModelFromUrlRequest,
) -> Result<String, String> {
    let logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .indexing
        .check_rate_limit("add_custom_model")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    // Input validation
    let validator = container.security_context().input_validator();
    validator
        .validate_model_name(&request.name)
        .map_err(|e| format!("Invalid model name: {}", e))?;
    validator
        .validate_model_name(&request.model_id)
        .map_err(|e| format!("Invalid model ID: {}", e))?;

    // Parse task type
    let task_type = parse_task_type(&request.task_type)?;

    // Parse architecture if provided
    let architecture = request
        .architecture
        .as_ref()
        .map(|a| parse_architecture(a))
        .transpose()?;

    // Create services
    let repository = Arc::new(CustomModelRepository::new(container.db_pool().clone()))
        as Arc<dyn CustomModelRepositoryTrait>;
    let url_validator = Arc::new(UrlValidationService::new().map_err(|e| e.to_string())?);
    let architecture_inferrer = Arc::new(ArchitectureInferenceService::new());

    // Create use case
    let use_case = AddFromUrlUseCase::new(
        repository,
        url_validator,
        architecture_inferrer,
        container.db_pool().clone(),
    );

    // Execute
    match use_case
        .execute(
            request.url.clone(),
            request.name.clone(),
            request.model_id.clone(),
            task_type,
            architecture,
        )
        .await
    {
        Ok(model_id) => {
            info!(model_id = %model_id, "Added custom model from URL");

            audit_success!(
                logger,
                AuditAction::AddCustomModel,
                &model_id,
                "url" => request.url.as_str(),
                "name" => request.name.as_str()
            )
            .await
            .ok();

            Ok(model_id)
        }
        Err(e) => {
            error!(error = %e, "Failed to add custom model from URL");

            audit_failure!(
                logger,
                AuditAction::AddCustomModel,
                &request.model_id,
                &e.to_string()
            )
            .await
            .ok();

            Err(e.to_string())
        }
    }
}

/// Add a custom model from file
///
/// Validates file (size, extension, path safety), copies to models directory,
/// and spawns background validation task.
pub async fn add_custom_model_from_file(
    container: State<'_, Container>,
    app_handle: tauri::AppHandle,
    request: AddModelFromFileRequest,
) -> Result<String, String> {
    let logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .indexing
        .check_rate_limit("add_custom_model")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    // Input validation
    let validator = container.security_context().input_validator();
    validator
        .validate_model_name(&request.name)
        .map_err(|e| format!("Invalid model name: {}", e))?;
    validator
        .validate_model_name(&request.model_id)
        .map_err(|e| format!("Invalid model ID: {}", e))?;

    // Parse task type
    let task_type = parse_task_type(&request.task_type)?;

    // Parse architecture if provided
    let architecture = request
        .architecture
        .as_ref()
        .map(|a| parse_architecture(a))
        .transpose()?;

    // Get models directory from app handle
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    let models_dir = app_data_dir.join("models");
    let repository = Arc::new(CustomModelRepository::new(container.db_pool().clone()))
        as Arc<dyn CustomModelRepositoryTrait>;
    let file_validator = Arc::new(FileValidationService::new(models_dir));
    let architecture_inferrer = Arc::new(ArchitectureInferenceService::new());

    // Create use case
    let use_case = AddFromFileUseCase::new(
        repository,
        file_validator,
        architecture_inferrer,
        container.db_pool().clone(),
    );

    // Execute
    match use_case
        .execute(
            request.file_path.clone(),
            request.name.clone(),
            request.model_id.clone(),
            task_type,
            architecture,
        )
        .await
    {
        Ok(model_id) => {
            info!(model_id = %model_id, "Added custom model from file");

            audit_success!(
                logger,
                AuditAction::AddCustomModel,
                &model_id,
                "file_path" => request.file_path.as_str(),
                "name" => request.name.as_str()
            )
            .await
            .ok();

            Ok(model_id)
        }
        Err(e) => {
            error!(error = %e, "Failed to add custom model from file");

            audit_failure!(
                logger,
                AuditAction::AddCustomModel,
                &request.model_id,
                &e.to_string()
            )
            .await
            .ok();

            Err(e.to_string())
        }
    }
}

/// List all custom models
///
/// Optionally filter by task type and/or validation status.
pub async fn list_custom_models(
    container: State<'_, Container>,
    task_type: Option<String>,
    validation_status: Option<String>,
) -> Result<Vec<CustomModelResponse>, String> {
    let logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("list_custom_models")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    // Parse filters
    let task_type_filter = task_type.as_ref().map(|t| parse_task_type(t)).transpose()?;
    let status_filter = validation_status
        .as_ref()
        .map(|s| parse_validation_status(s))
        .transpose()?;

    // Create repository and use case
    let repository = Arc::new(CustomModelRepository::new(container.db_pool().clone()))
        as Arc<dyn CustomModelRepositoryTrait>
        as Arc<dyn CustomModelRepositoryTrait>;
    let use_case = ListUseCase::new(repository);

    // Execute
    match use_case
        .execute_filtered(task_type_filter, status_filter)
        .await
    {
        Ok(models) => {
            let response: Vec<CustomModelResponse> = models.into_iter().map(|m| m.into()).collect();

            info!(count = response.len(), "Listed custom models");

            audit_success!(
                logger,
                AuditAction::ModelViewed,
                "custom_models",
                "count" => response.len().to_string().as_str()
            )
            .await
            .ok();

            Ok(response)
        }
        Err(e) => {
            error!(error = %e, "Failed to list custom models");
            Err(e.to_string())
        }
    }
}

/// Get a single custom model by ID
pub async fn get_custom_model_by_id(
    container: State<'_, Container>,
    model_id: String,
) -> Result<CustomModelResponse, String> {
    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("get_custom_model")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    // Create repository
    let repository = CustomModelRepository::new(container.db_pool().clone());

    // Execute - find by model_id (not UUID id)
    match repository.find_by_model_id(&model_id).await {
        Ok(Some(model)) => {
            info!(model_id = %model_id, "Retrieved custom model");
            Ok(model.into())
        }
        Ok(None) => {
            error!(model_id = %model_id, "Custom model not found");
            Err(format!("Custom model {} not found", model_id))
        }
        Err(e) => {
            error!(error = %e, model_id = %model_id, "Failed to get custom model");
            Err(e.to_string())
        }
    }
}

/// Delete a custom model
///
/// Optionally delete the file from disk as well.
pub async fn delete_custom_model(
    container: State<'_, Container>,
    app_handle: tauri::AppHandle,
    model_id: String,
    delete_file: bool,
) -> Result<(), String> {
    let logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .indexing
        .check_rate_limit("delete_custom_model")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    // Get models directory from app handle
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    let models_dir = app_data_dir.join("models");
    let repository = Arc::new(CustomModelRepository::new(container.db_pool().clone()))
        as Arc<dyn CustomModelRepositoryTrait>;
    let file_validator = Arc::new(FileValidationService::new(models_dir));

    // Create use case
    let use_case = DeleteUseCase::new(repository, file_validator);

    // Execute
    match use_case.execute(model_id.clone(), delete_file).await {
        Ok(_) => {
            info!(model_id = %model_id, deleted_file = delete_file, "Deleted custom model");

            audit_success!(
                logger,
                AuditAction::DeleteCustomModel,
                &model_id,
                "deleted_file" => delete_file.to_string().as_str()
            )
            .await
            .ok();

            Ok(())
        }
        Err(e) => {
            error!(error = %e, model_id = %model_id, "Failed to delete custom model");

            audit_failure!(
                logger,
                AuditAction::DeleteCustomModel,
                &model_id,
                &e.to_string()
            )
            .await
            .ok();

            Err(e.to_string())
        }
    }
}

/// Validate a custom model
///
/// Triggers deep validation (file signature verification) and updates status.
pub async fn validate_custom_model(
    container: State<'_, Container>,
    app_handle: tauri::AppHandle,
    model_id: String,
) -> Result<String, String> {
    let logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .indexing
        .check_rate_limit("validate_custom_model")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    // Get models directory from app handle
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    let models_dir = app_data_dir.join("models");
    let repository = Arc::new(CustomModelRepository::new(container.db_pool().clone()))
        as Arc<dyn CustomModelRepositoryTrait>;
    let url_validator = Arc::new(UrlValidationService::new().map_err(|e| e.to_string())?);
    let file_validator = Arc::new(FileValidationService::new(models_dir));

    // Create use case
    let use_case = ValidateUseCase::new(repository, url_validator, file_validator);

    // Execute
    match use_case.execute(model_id.clone()).await {
        Ok(status) => {
            let status_str = format!("{:?}", status);
            info!(model_id = %model_id, status = %status_str, "Validated custom model");

            audit_success!(
                logger,
                AuditAction::ValidateCustomModel,
                &model_id,
                "status" => status_str.as_str()
            )
            .await
            .ok();

            Ok(status_str)
        }
        Err(e) => {
            error!(error = %e, model_id = %model_id, "Failed to validate custom model");

            audit_failure!(
                logger,
                AuditAction::ValidateCustomModel,
                &model_id,
                &e.to_string()
            )
            .await
            .ok();

            Err(e.to_string())
        }
    }
}

// Helper functions for parsing enums

fn parse_task_type(s: &str) -> Result<TaskType, String> {
    match s.to_lowercase().as_str() {
        "embedding" => Ok(TaskType::Embedding),
        "chat" => Ok(TaskType::Chat),
        "ocr" => Ok(TaskType::Ocr),
        "vision" => Ok(TaskType::Vision),
        _ => Err(format!(
            "Invalid task type: {}. Must be one of: embedding, chat, ocr, vision",
            s
        )),
    }
}

fn parse_architecture(s: &str) -> Result<ModelArchitecture, String> {
    match s.to_uppercase().as_str() {
        "GGUF" => Ok(ModelArchitecture::Gguf),
        "ONNX" => Ok(ModelArchitecture::Onnx),
        "SAFETENSORS" => Ok(ModelArchitecture::SafeTensors),
        "PYTORCH" => Ok(ModelArchitecture::PyTorch),
        _ => Err(format!(
            "Invalid architecture: {}. Must be one of: GGUF, ONNX, SafeTensors, PyTorch",
            s
        )),
    }
}

fn parse_validation_status(s: &str) -> Result<ValidationStatus, String> {
    match s.to_lowercase().as_str() {
        "pending" => Ok(ValidationStatus::Pending),
        "valid" => Ok(ValidationStatus::Valid),
        "invalid" => Ok(ValidationStatus::Invalid),
        _ => Err(format!(
            "Invalid validation status: {}. Must be one of: pending, valid, invalid",
            s
        )),
    }
}
