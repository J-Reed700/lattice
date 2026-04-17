//! Custom Model Repository
//!
//! Manages persistence of custom model records in SQLite.

use crate::domain::custom_model::{
    CustomModel, FileInfo, ModelArchitecture, ModelId, ModelMetadata, ModelName, ModelSource,
    SourceType, TaskType, ValidationStatus,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};
use uuid::Uuid;

// Database row struct for query results
#[derive(Debug, sqlx::FromRow)]
struct CustomModelRow {
    id: String,
    name: String,
    model_id: String,
    source_type: String,
    source_value: String,
    file_path: String,
    file_size_bytes: i64,
    architecture: Option<String>,
    task_type: String,
    validation_status: String,
    validation_error: Option<String>,
    metadata_json: Option<String>,
    created_at: String,
    updated_at: String,
    last_validated_at: Option<String>,
}

impl TryFrom<CustomModelRow> for CustomModel {
    type Error = AppError;

    fn try_from(row: CustomModelRow) -> Result<Self> {
        let name = ModelName::new(row.name)
            .map_err(|e| AppError::InvalidData(format!("Invalid model name: {}", e)))?;
        let model_id = ModelId::new(row.model_id)
            .map_err(|e| AppError::InvalidData(format!("Invalid model_id: {}", e)))?;

        let source_type_enum = SourceType::from_db_string(&row.source_type)
            .map_err(|e| AppError::InvalidData(format!("Invalid source_type: {}", e)))?;
        let source = match source_type_enum {
            SourceType::Url => ModelSource::Url(row.source_value),
            SourceType::LocalFile => ModelSource::LocalFile(PathBuf::from(row.source_value)),
        };

        let architecture_enum = if let Some(arch_str) = row.architecture {
            Some(
                ModelArchitecture::from_db_string(&arch_str)
                    .map_err(|e| AppError::InvalidData(format!("Invalid architecture: {}", e)))?,
            )
        } else {
            None
        };

        let task_type_enum = TaskType::from_db_string(&row.task_type)
            .map_err(|e| AppError::InvalidData(format!("Invalid task_type: {}", e)))?;

        let validation_status_enum = ValidationStatus::from_db_string(&row.validation_status)
            .map_err(|e| AppError::InvalidData(format!("Invalid validation_status: {}", e)))?;

        let metadata = if let Some(json) = row.metadata_json {
            Some(
                ModelMetadata::from_json(&json)
                    .map_err(|e| AppError::InvalidData(format!("Invalid metadata JSON: {}", e)))?,
            )
        } else {
            None
        };

        let created_at_dt = DateTime::parse_from_rfc3339(&row.created_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid created_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let updated_at_dt = DateTime::parse_from_rfc3339(&row.updated_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid updated_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let last_validated_at_dt = if let Some(ts) = row.last_validated_at {
            Some(
                DateTime::parse_from_rfc3339(&ts)
                    .map_err(|e| {
                        AppError::InvalidData(format!("Invalid last_validated_at timestamp: {}", e))
                    })?
                    .with_timezone(&Utc),
            )
        } else {
            None
        };

        CustomModel::from_db(
            row.id,
            name,
            model_id,
            source,
            PathBuf::from(row.file_path),
            row.file_size_bytes,
            architecture_enum,
            task_type_enum,
            validation_status_enum,
            row.validation_error,
            metadata,
            created_at_dt,
            updated_at_dt,
            last_validated_at_dt,
        )
        .map_err(|e| AppError::InvalidData(format!("Failed to reconstruct CustomModel: {}", e)))
    }
}

// ============================================================================
// Repository Trait
// ============================================================================

/// Trait for custom model storage operations
///
/// Defines the contract for storing and retrieving custom models from persistent storage.
/// Custom models represent user-added models (URL downloads or file uploads).
///
/// # Implementations
/// - `CustomModelRepository`: Production SQLite implementation
/// - `MockCustomModelRepository`: In-memory mock for testing
#[async_trait]
pub trait CustomModelRepositoryTrait: Send + Sync {
    /// Save a custom model (insert or update)
    ///
    /// Uses UPSERT to handle both insert and update cases.
    async fn save(&self, model: &CustomModel) -> Result<()>;

    /// Find a custom model by its record ID
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<CustomModel>>;

    /// Find a custom model by its model_id (unique identifier)
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<CustomModel>>;

    /// List all custom models
    async fn find_all(&self) -> Result<Vec<CustomModel>>;

    /// Find custom models by task type
    async fn find_by_task_type(&self, task_type: TaskType) -> Result<Vec<CustomModel>>;

    /// Find custom models by validation status
    async fn find_by_validation_status(&self, status: ValidationStatus)
        -> Result<Vec<CustomModel>>;

    /// Delete a custom model by ID
    async fn delete(&self, id: &Uuid) -> Result<()>;

    /// Update validation status (for validation workflows)
    async fn update_validation_status(
        &self,
        id: &Uuid,
        status: ValidationStatus,
        error: Option<String>,
    ) -> Result<()>;
}

// ============================================================================
// SQLite Implementation
// ============================================================================

/// Production SQLite implementation of CustomModelRepositoryTrait
#[derive(Clone)]
pub struct CustomModelRepository {
    pool: SqlitePool,
}

impl CustomModelRepository {
    /// Create a new repository instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

// SQLite repository implementation
// Note: Requires DATABASE_URL environment variable for sqlx compile-time checking
#[async_trait]
impl CustomModelRepositoryTrait for CustomModelRepository {
    async fn save(&self, model: &CustomModel) -> Result<()> {
        // Extract all fields for binding
        let id = model.id().to_string();
        let name = model.name().as_str();
        let model_id_str = model.model_id().as_str();
        let source_type = model.source().source_type().to_db_string();
        let source_value = model.source().source_value();
        let file_path = model
            .file_info()
            .path()
            .to_str()
            .ok_or_else(|| AppError::InvalidInput("Invalid file path encoding".to_string()))?;
        let file_size_bytes = model.file_info().size_bytes();
        let architecture = model.architecture().map(|a| a.to_db_string());
        let task_type = model.task_type().to_db_string();
        let validation_status = model.validation_status().to_db_string();
        let validation_error = model.validation_error();
        let metadata_json = model.metadata().map(|m| m.to_json());
        let created_at = model.created_at().to_rfc3339();
        let updated_at = model.updated_at().to_rfc3339();
        let last_validated_at = model.last_validated_at().map(|dt| dt.to_rfc3339());

        sqlx::query!(
            r#"
            INSERT INTO custom_models (
                id, name, model_id, source_type, source_value,
                file_path, file_size_bytes, architecture, task_type,
                validation_status, validation_error, metadata_json,
                created_at, updated_at, last_validated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                model_id = excluded.model_id,
                source_type = excluded.source_type,
                source_value = excluded.source_value,
                file_path = excluded.file_path,
                file_size_bytes = excluded.file_size_bytes,
                architecture = excluded.architecture,
                task_type = excluded.task_type,
                validation_status = excluded.validation_status,
                validation_error = excluded.validation_error,
                metadata_json = excluded.metadata_json,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                last_validated_at = excluded.last_validated_at
            "#,
            id,
            name,
            model_id_str,
            source_type,
            source_value,
            file_path,
            file_size_bytes,
            architecture,
            task_type,
            validation_status,
            validation_error,
            metadata_json,
            created_at,
            updated_at,
            last_validated_at
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id_str, "Failed to save custom model");
            AppError::Database(format!("Failed to save custom model: {}", e))
        })?;

        debug!(model_id = %model_id_str, "Custom model saved successfully");
        Ok(())
    }

    async fn find_by_id(&self, id: &Uuid) -> Result<Option<CustomModel>> {
        let id_str = id.to_string();

        let record = sqlx::query_as::<_, CustomModelRow>(
            r#"
            SELECT id, name, model_id, source_type, source_value,
                   file_path, file_size_bytes, architecture, task_type,
                   validation_status, validation_error, metadata_json,
                   created_at, updated_at, last_validated_at
            FROM custom_models
            WHERE id = ?
            "#,
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, id = %id_str, "Failed to find custom model by id");
            AppError::Database(format!("Failed to find custom model: {}", e))
        })?;

        match record {
            Some(row) => Ok(Some(row.try_into()?)),
            None => Ok(None),
        }
    }

    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<CustomModel>> {
        let record = sqlx::query_as::<_, CustomModelRow>(
            r#"
            SELECT id, name, model_id, source_type, source_value,
                   file_path, file_size_bytes, architecture, task_type,
                   validation_status, validation_error, metadata_json,
                   created_at, updated_at, last_validated_at
            FROM custom_models
            WHERE model_id = ?
            "#,
        )
        .bind(model_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to find custom model by model_id");
            AppError::Database(format!("Failed to find custom model: {}", e))
        })?;

        match record {
            Some(row) => Ok(Some(row.try_into()?)),
            None => Ok(None),
        }
    }

    async fn find_all(&self) -> Result<Vec<CustomModel>> {
        let records = sqlx::query_as::<_, CustomModelRow>(
            r#"
            SELECT id, name, model_id, source_type, source_value,
                   file_path, file_size_bytes, architecture, task_type,
                   validation_status, validation_error, metadata_json,
                   created_at, updated_at, last_validated_at
            FROM custom_models
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to list all custom models");
            AppError::Database(format!("Failed to list custom models: {}", e))
        })?;

        let models = records
            .into_iter()
            .filter_map(|row| match row.try_into() {
                Ok(model) => Some(model),
                Err(e) => {
                    error!(error = %e, "Failed to parse custom model row");
                    None
                }
            })
            .collect();

        Ok(models)
    }

    async fn find_by_task_type(&self, task_type: TaskType) -> Result<Vec<CustomModel>> {
        let task_type_str = task_type.to_db_string();

        let records = sqlx::query_as::<_, CustomModelRow>(
            r#"
            SELECT id, name, model_id, source_type, source_value,
                   file_path, file_size_bytes, architecture, task_type,
                   validation_status, validation_error, metadata_json,
                   created_at, updated_at, last_validated_at
            FROM custom_models
            WHERE task_type = ?
            ORDER BY created_at DESC
            "#
        )
        .bind(task_type_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, task_type = %task_type_str, "Failed to find custom models by task_type");
            AppError::Database(format!("Failed to find custom models: {}", e))
        })?;

        let models = records
            .into_iter()
            .filter_map(|row| match row.try_into() {
                Ok(model) => Some(model),
                Err(e) => {
                    error!(error = %e, "Failed to parse custom model row");
                    None
                }
            })
            .collect();

        Ok(models)
    }

    async fn find_by_validation_status(
        &self,
        status: ValidationStatus,
    ) -> Result<Vec<CustomModel>> {
        let status_str = status.to_db_string();

        let records = sqlx::query_as::<_, CustomModelRow>(
            r#"
            SELECT id, name, model_id, source_type, source_value,
                   file_path, file_size_bytes, architecture, task_type,
                   validation_status, validation_error, metadata_json,
                   created_at, updated_at, last_validated_at
            FROM custom_models
            WHERE validation_status = ?
            ORDER BY created_at DESC
            "#
        )
        .bind(status_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, validation_status = %status_str, "Failed to find custom models by validation_status");
            AppError::Database(format!("Failed to find custom models: {}", e))
        })?;

        let models = records
            .into_iter()
            .filter_map(|row| match row.try_into() {
                Ok(model) => Some(model),
                Err(e) => {
                    error!(error = %e, "Failed to parse custom model row");
                    None
                }
            })
            .collect();

        Ok(models)
    }

    async fn delete(&self, id: &Uuid) -> Result<()> {
        let id_str = id.to_string();

        sqlx::query!(
            r#"
            DELETE FROM custom_models
            WHERE id = ?1
            "#,
            id_str
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, id = %id_str, "Failed to delete custom model");
            AppError::Database(format!("Failed to delete custom model: {}", e))
        })?;

        debug!(id = %id_str, "Custom model deleted successfully");
        Ok(())
    }

    async fn update_validation_status(
        &self,
        id: &Uuid,
        status: ValidationStatus,
        error: Option<String>,
    ) -> Result<()> {
        let id_str = id.to_string();
        let status_str = status.to_db_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query!(
            r#"
            UPDATE custom_models
            SET validation_status = ?1,
                validation_error = ?2,
                last_validated_at = ?3
            WHERE id = ?4
            "#,
            status_str,
            error,
            now,
            id_str
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, id = %id_str, "Failed to update validation status");
            AppError::Database(format!("Failed to update validation status: {}", e))
        })?;

        debug!(id = %id_str, status = %status_str, "Validation status updated");
        Ok(())
    }
}

// ============================================================================
// Mock Implementation (for testing)
// ============================================================================

/// In-memory mock implementation for testing
pub struct MockCustomModelRepository {
    models: Arc<Mutex<HashMap<Uuid, CustomModel>>>,
}

impl MockCustomModelRepository {
    /// Create a new mock repository
    pub fn new() -> Self {
        Self {
            models: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for MockCustomModelRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CustomModelRepositoryTrait for MockCustomModelRepository {
    async fn save(&self, model: &CustomModel) -> Result<()> {
        let mut models = self.models.lock().await;
        let id = Uuid::parse_str(model.id())
            .map_err(|e| AppError::InvalidData(format!("Invalid UUID in model ID: {}", e)))?;
        models.insert(id, model.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: &Uuid) -> Result<Option<CustomModel>> {
        let models = self.models.lock().await;
        Ok(models.get(id).cloned())
    }

    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<CustomModel>> {
        let models = self.models.lock().await;
        Ok(models
            .values()
            .find(|m| m.model_id().as_str() == model_id)
            .cloned())
    }

    async fn find_all(&self) -> Result<Vec<CustomModel>> {
        let models = self.models.lock().await;
        let mut all: Vec<CustomModel> = models.values().cloned().collect();
        all.sort_by(|a, b| b.created_at().cmp(a.created_at()));
        Ok(all)
    }

    async fn find_by_task_type(&self, task_type: TaskType) -> Result<Vec<CustomModel>> {
        let models = self.models.lock().await;
        let mut filtered: Vec<CustomModel> = models
            .values()
            .filter(|m| m.task_type() == task_type)
            .cloned()
            .collect();
        filtered.sort_by(|a, b| b.created_at().cmp(a.created_at()));
        Ok(filtered)
    }

    async fn find_by_validation_status(
        &self,
        status: ValidationStatus,
    ) -> Result<Vec<CustomModel>> {
        let models = self.models.lock().await;
        let mut filtered: Vec<CustomModel> = models
            .values()
            .filter(|m| m.validation_status() == status)
            .cloned()
            .collect();
        filtered.sort_by(|a, b| b.created_at().cmp(a.created_at()));
        Ok(filtered)
    }

    async fn delete(&self, id: &Uuid) -> Result<()> {
        let mut models = self.models.lock().await;
        models.remove(id);
        Ok(())
    }

    async fn update_validation_status(
        &self,
        id: &Uuid,
        status: ValidationStatus,
        error: Option<String>,
    ) -> Result<()> {
        let mut models = self.models.lock().await;
        if let Some(model) = models.get_mut(id) {
            // Clone, modify, and reinsert (workaround for mut borrow)
            let mut updated = model.clone();
            match status {
                ValidationStatus::Pending => {}
                ValidationStatus::Validating => updated.mark_validating(),
                ValidationStatus::Valid => updated.mark_valid(),
                ValidationStatus::Invalid => {
                    updated.mark_invalid(error.unwrap_or_else(|| "Unknown error".to_string()))
                }
            }
            models.insert(*id, updated);
        }
        Ok(())
    }
}
