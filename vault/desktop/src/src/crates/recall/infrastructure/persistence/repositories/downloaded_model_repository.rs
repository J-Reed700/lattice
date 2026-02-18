//! Downloaded Model Repository
//!
//! Manages persistence of downloaded model records in SQLite.

use crate::domain::downloaded_model::{DownloadedModel, ModelType};
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// Database record structure for RETURNING queries
#[derive(Debug, sqlx::FromRow)]
struct DownloadedModelRecord {
    id: String,
    model_name: String,
    model_id: String,
    file_path: String,
    file_size_bytes: i64,
    model_type: String,
    architecture: String,
    downloaded_at: Option<String>,
    last_used_at: Option<String>,
    use_count: i64,
    is_active_for_chat: i64,
    is_active_for_embedding: i64,
    metadata: Option<String>,
}

const MODEL_FILE_SUMMARY_CTE: &str = r#"
WITH ranked_model_files AS (
    SELECT
        mf.model_id,
        mf.file_path,
        mf.file_name,
        ROW_NUMBER() OVER (
            PARTITION BY mf.model_id
            ORDER BY
                CASE
                    WHEN mf.file_name = 'model.onnx' THEN 0
                    WHEN mf.file_name LIKE '%.onnx' AND mf.file_name NOT LIKE '%.onnx_data' THEN 1
                    WHEN mf.file_name LIKE '%.gguf' THEN 2
                    ELSE 3
                END,
                mf.file_name
        ) AS rn
    FROM model_files mf
),
primary_model_files AS (
    SELECT model_id, file_path AS primary_file_path
    FROM ranked_model_files
    WHERE rn = 1
),
model_file_totals AS (
    SELECT model_id, SUM(size_bytes) AS total_file_size_bytes
    FROM model_files
    GROUP BY model_id
)
"#;

const MODEL_SELECT_BASE: &str = r#"
SELECT
    m.id,
    m.model_name,
    m.model_id,
    COALESCE(pmf.primary_file_path, m.base_path) AS file_path,
    COALESCE(mft.total_file_size_bytes, m.total_size_bytes, 0) AS file_size_bytes,
    m.model_type,
    m.architecture,
    m.downloaded_at,
    m.last_used_at,
    m.use_count,
    m.is_active_for_chat,
    m.is_active_for_embedding,
    m.metadata
FROM models m
LEFT JOIN primary_model_files pmf ON pmf.model_id = m.model_id
LEFT JOIN model_file_totals mft ON mft.model_id = m.model_id
"#;

/// Map domain model type strings to the models-table CHECK constraint values.
///
/// The domain uses modern values (`language_model`, `text_embeddings`), while the
/// legacy `models` table constraint still expects (`chat`, `embedding`, ...).
fn normalize_model_type_for_models_table(model_type: ModelType) -> &'static str {
    match model_type {
        ModelType::LanguageModel => "chat",
        ModelType::TextEmbeddings => "embedding",
        ModelType::Vision => "multi_modal",
        ModelType::Reranker => "qa",
    }
}

/// Convert database record to domain entity
impl TryFrom<DownloadedModelRecord> for DownloadedModel {
    type Error = AppError;

    fn try_from(record: DownloadedModelRecord) -> Result<Self, Self::Error> {
        // Parse model type enum
        let model_type_enum = ModelType::from_db_string(&record.model_type)
            .map_err(|e| AppError::InvalidData(format!("Invalid model type: {}", e)))?;

        // Parse timestamps
        let downloaded_at_dt =
            parse_optional_db_timestamp(record.downloaded_at.as_deref())?.unwrap_or_else(Utc::now);
        let last_used_at_dt = parse_optional_db_timestamp(record.last_used_at.as_deref())?;

        // Parse metadata JSON
        let metadata_val = DownloadedModel::metadata_from_json(record.metadata.as_deref())
            .map_err(|e| AppError::InvalidData(format!("Invalid metadata JSON: {}", e)))?;

        // Convert is_active flags from integer to bool
        let is_active_chat = record.is_active_for_chat != 0;
        let is_active_embedding = record.is_active_for_embedding != 0;

        Ok(DownloadedModel::from_db(
            record.id,
            record.model_name,
            record.model_id,
            PathBuf::from(record.file_path),
            record.file_size_bytes,
            model_type_enum,
            record.architecture,
            downloaded_at_dt,
            last_used_at_dt,
            record.use_count,
            is_active_chat,
            is_active_embedding,
            metadata_val,
        ))
    }
}

/// Repository for downloaded model persistence
///
/// Provides CRUD operations for downloaded model records.
/// Handles conversion between domain entities and database rows.
#[derive(Clone)]
pub struct DownloadedModelRepository {
    pool: SqlitePool,
}

impl DownloadedModelRepository {
    /// Create a new repository instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Save a downloaded model (insert or update)
    ///
    /// # Arguments
    ///
    /// * `model` - The downloaded model to save
    ///
    /// # Business Logic
    ///
    /// - Uses UPSERT to handle both insert and update
    /// - Updates all fields on conflict
    /// - If setting is_active_for_chat=1, trigger deactivates other models
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails
    pub async fn save(&self, model: &DownloadedModel) -> Result<()> {
        // Bind all values to variables to satisfy borrow checker
        let id = model.id().to_string();
        let model_name = model.model_name().to_string();
        let model_id = model.model_id().to_string();
        let file_path_str = model
            .file_path()
            .to_str()
            .ok_or_else(|| AppError::InvalidInput("Invalid file path encoding".to_string()))?;
        // Extract base_path (directory) from file_path for database storage
        let base_path = model
            .file_path()
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_size_bytes = model.file_size_bytes();
        let model_type_str = normalize_model_type_for_models_table(model.model_type()).to_string();
        let architecture = model.architecture().to_string();
        let downloaded_at = model.downloaded_at().to_rfc3339();
        let last_used_at = model.last_used_at().map(|dt| dt.to_rfc3339());
        let use_count = model.use_count();
        let is_active_chat = if model.is_active_for_chat() { 1 } else { 0 };
        let is_active_embedding = if model.is_active_for_embedding() {
            1
        } else {
            0
        };
        let metadata_json = model.metadata_to_json();
        let total_size_bytes = model.file_size_bytes();

        let mut tx = self.pool.begin().await.map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to begin transaction for model save");
            AppError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        sqlx::query(
            r#"
            INSERT INTO models (
                id, model_name, model_id, base_path, total_size_bytes, status, model_type, architecture,
                downloaded_at, last_used_at, use_count, is_active_for_chat, is_active_for_embedding, metadata
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'completed', ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(model_id) DO UPDATE SET
                model_name = excluded.model_name,
                base_path = excluded.base_path,
                total_size_bytes = excluded.total_size_bytes,
                status = excluded.status,
                model_type = excluded.model_type,
                architecture = excluded.architecture,
                downloaded_at = excluded.downloaded_at,
                last_used_at = excluded.last_used_at,
                use_count = excluded.use_count,
                is_active_for_chat = excluded.is_active_for_chat,
                is_active_for_embedding = excluded.is_active_for_embedding,
                metadata = excluded.metadata
            "#,
        )
        .bind(id)
        .bind(model_name)
        .bind(&model_id)
        .bind(base_path)
        .bind(total_size_bytes)
        .bind(model_type_str)
        .bind(architecture)
        .bind(downloaded_at)
        .bind(last_used_at)
        .bind(use_count)
        .bind(is_active_chat)
        .bind(is_active_embedding)
        .bind(metadata_json)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to save downloaded model");
            AppError::Database(format!("Failed to save downloaded model: {}", e))
        })?;

        // Compatibility for code paths that save a model directly (without pre-created files):
        // if no model_files rows exist, synthesize a single completed file row.
        let existing_file_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM model_files WHERE model_id = ?1")
                .bind(&model_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| {
                    error!(error = %e, model_id = %model_id, "Failed to count model files");
                    AppError::Database(format!("Failed to count model files: {}", e))
                })?;

        if existing_file_count == 0 {
            let file_name = model
                .file_path()
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}-model", model_id));
            let now = Utc::now().to_rfc3339();
            let model_file_id = uuid::Uuid::new_v4().to_string();

            sqlx::query(
                r#"
                INSERT INTO model_files (
                    id, model_id, file_name, file_path, relative_path, size_bytes, downloaded_bytes,
                    checksum_sha256, download_url, status, created_at, updated_at, downloaded_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, 'completed', ?8, ?9, ?10)
                ON CONFLICT(model_id, file_name) DO UPDATE SET
                    file_path = excluded.file_path,
                    relative_path = excluded.relative_path,
                    size_bytes = excluded.size_bytes,
                    downloaded_bytes = excluded.downloaded_bytes,
                    status = excluded.status,
                    updated_at = excluded.updated_at,
                    downloaded_at = excluded.downloaded_at
                "#,
            )
            .bind(model_file_id)
            .bind(&model_id)
            .bind(&file_name)
            .bind(file_path_str)
            .bind(&file_name)
            .bind(file_size_bytes)
            .bind(file_size_bytes)
            .bind(&now)
            .bind(&now)
            .bind(model.downloaded_at().to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                error!(error = %e, model_id = %model_id, "Failed to upsert fallback model_file");
                AppError::Database(format!("Failed to upsert model file: {}", e))
            })?;
        }

        tx.commit().await.map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to commit model save transaction");
            AppError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        debug!(model_id = %model_id, "Downloaded model saved successfully");
        Ok(())
    }

    /// Find a downloaded model by its record ID
    ///
    /// # Arguments
    ///
    /// * `id` - The unique record ID
    ///
    /// # Returns
    ///
    /// Some(DownloadedModel) if found, None otherwise
    pub async fn find_by_id(&self, id: &str) -> Result<Option<DownloadedModel>> {
        let query = build_model_select_query("WHERE m.id = ?1");
        let record = sqlx::query_as::<_, DownloadedModelRecord>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!(error = %e, id = %id, "Failed to find downloaded model by id");
                AppError::Database(format!("Failed to find downloaded model: {}", e))
            })?;

        match record {
            Some(row) => Ok(Some(row.try_into()?)),
            None => Ok(None),
        }
    }

    /// Find a downloaded model by its model_id
    ///
    /// # Arguments
    ///
    /// * `model_id` - The unique model identifier
    ///
    /// # Returns
    ///
    /// Some(DownloadedModel) if found, None otherwise
    pub async fn find_by_model_id(&self, model_id: &str) -> Result<Option<DownloadedModel>> {
        let query = build_model_select_query("WHERE m.model_id = ?1");
        let record = sqlx::query_as::<_, DownloadedModelRecord>(&query)
        .bind(model_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to find downloaded model by model_id");
            AppError::Database(format!("Failed to find downloaded model: {}", e))
        })?;

        match record {
            Some(row) => Ok(Some(row.try_into()?)),
            None => Ok(None),
        }
    }

    /// List all downloaded models
    ///
    /// # Returns
    ///
    /// Vector of all downloaded models, ordered by download date (newest first)
    pub async fn list_all(&self) -> Result<Vec<DownloadedModel>> {
        let query = build_model_select_query(
            "WHERE m.status = 'completed' ORDER BY m.downloaded_at DESC, m.created_at DESC",
        );
        let records = sqlx::query_as::<_, DownloadedModelRecord>(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to list downloaded models");
                AppError::Database(format!("Failed to list downloaded models: {}", e))
            })?;

        let models = records
            .into_iter()
            .filter_map(|row| match row.try_into() {
                Ok(model) => Some(model),
                Err(e) => {
                    error!(error = %e, "Failed to convert downloaded model record");
                    None
                }
            })
            .collect();

        Ok(models)
    }

    /// Get the currently active chat model
    ///
    /// # Returns
    ///
    /// Some(DownloadedModel) if an active chat model exists, None otherwise
    ///
    /// # Business Logic
    ///
    /// - Database trigger ensures only one model has is_active_for_chat=1
    /// - This query should return at most one result
    pub async fn get_active_chat_model(&self) -> Result<Option<DownloadedModel>> {
        let query = build_model_select_query("WHERE m.is_active_for_chat = 1 LIMIT 1");
        let record = sqlx::query_as::<_, DownloadedModelRecord>(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to get active chat model");
                AppError::Database(format!("Failed to get active chat model: {}", e))
            })?;

        match record {
            Some(row) => {
                let model: DownloadedModel = row.try_into()?;
                if self.is_downloaded(model.model_id()).await? {
                    Ok(Some(model))
                } else {
                    warn!(
                        model_id = %model.model_id(),
                        "Active chat model is not fully downloaded; ignoring active selection"
                    );
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Get the currently active embedding model
    ///
    /// # Returns
    ///
    /// Some(DownloadedModel) if an active embedding model exists, None otherwise
    ///
    /// # Business Logic
    ///
    /// - Database trigger ensures only one model has is_active_for_embedding=1
    /// - This query should return at most one result
    pub async fn get_active_embedding_model(&self) -> Result<Option<DownloadedModel>> {
        let query = build_model_select_query("WHERE m.is_active_for_embedding = 1 LIMIT 1");
        let record = sqlx::query_as::<_, DownloadedModelRecord>(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to get active embedding model");
                AppError::Database(format!("Failed to get active embedding model: {}", e))
            })?;

        match record {
            Some(row) => {
                let model: DownloadedModel = row.try_into()?;
                if self.is_downloaded(model.model_id()).await? {
                    Ok(Some(model))
                } else {
                    warn!(
                        model_id = %model.model_id(),
                        "Active embedding model is not fully downloaded; ignoring active selection"
                    );
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Set a model as the active chat model
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model_id to set as active
    ///
    /// # Business Logic
    ///
    /// - Sets is_active_for_chat=1 for the specified model
    /// - Database trigger automatically sets is_active_for_chat=0 for all others
    /// - Fails if model_id doesn't exist
    ///
    /// # Errors
    ///
    /// Returns NotFound error if model_id doesn't exist
    pub async fn set_active_chat_model(&self, model_id: &str) -> Result<()> {
        let result = sqlx::query!(
            r#"
            UPDATE models
            SET is_active_for_chat = 1
            WHERE model_id = ?1
            "#,
            model_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to set active chat model");
            AppError::Database(format!("Failed to set active chat model: {}", e))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
        }

        info!(model_id = %model_id, "Set active chat model");
        Ok(())
    }

    /// Set a model as the active embedding model
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model_id to set as active
    ///
    /// # Business Logic
    ///
    /// - Sets is_active_for_embedding=1 for the specified model
    /// - Database trigger automatically sets is_active_for_embedding=0 for all others
    /// - Fails if model_id doesn't exist
    ///
    /// # Errors
    ///
    /// Returns NotFound error if model_id doesn't exist
    pub async fn set_active_embedding_model(&self, model_id: &str) -> Result<()> {
        let result = sqlx::query!(
            r#"
            UPDATE models
            SET is_active_for_embedding = 1
            WHERE model_id = ?1
            "#,
            model_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to set active embedding model");
            AppError::Database(format!("Failed to set active embedding model: {}", e))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
        }

        info!(model_id = %model_id, "Set active embedding model");
        Ok(())
    }

    /// Clear the active chat model (set all to inactive)
    pub async fn clear_active_chat_model(&self) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE models
            SET is_active_for_chat = 0
            WHERE is_active_for_chat = 1
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to clear active chat model");
            AppError::Database(format!("Failed to clear active chat model: {}", e))
        })?;

        info!("Active chat model cleared");
        Ok(())
    }

    /// Clear the active embedding model (set all to inactive)
    pub async fn clear_active_embedding_model(&self) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE models
            SET is_active_for_embedding = 0
            WHERE is_active_for_embedding = 1
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to clear active embedding model");
            AppError::Database(format!("Failed to clear active embedding model: {}", e))
        })?;

        info!("Active embedding model cleared");
        Ok(())
    }

    /// Atomically deletes a model ONLY if it is NOT active
    ///
    /// # Arguments
    ///
    /// * `id` - The unique model_id (NOT record ID)
    ///
    /// # Returns
    ///
    /// - Ok(DownloadedModel) if model was deleted (was not active)
    /// - Err(NotFound) if model doesn't exist
    /// - Err(InvalidInput) if model is currently active (chat or embedding)
    ///
    /// # Security
    ///
    /// This method fixes CWE-367 (TOCTOU race condition) by using a single
    /// atomic DELETE query with WHERE guard. The check and deletion happen
    /// in one database operation, eliminating the race window.
    ///
    /// # Business Logic
    ///
    /// - Uses DELETE ... WHERE is_active_for_chat = 0 AND is_active_for_embedding = 0
    /// - Returns the deleted row via RETURNING clause
    /// - If no row deleted, queries again to distinguish "not found" vs "active"
    pub async fn delete_if_not_active(&self, id: &str) -> Result<Option<DownloadedModel>> {
        // Atomic DELETE with WHERE guard (single statement; no read/delete race window).
        let result = sqlx::query_as::<_, DownloadedModelRecord>(
            r#"
            DELETE FROM models
            WHERE id = ?1
              AND is_active_for_chat = 0
              AND is_active_for_embedding = 0
            RETURNING
              id,
              model_name,
              model_id,
              base_path AS file_path,
              total_size_bytes AS file_size_bytes,
              model_type,
              architecture,
              downloaded_at,
              last_used_at,
              use_count,
              is_active_for_chat,
              is_active_for_embedding,
              metadata
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, id = %id, "Failed to atomically delete model");
            AppError::Database(format!("Failed to delete model: {}", e))
        })?;

        match result {
            Some(record) => {
                // Model was NOT active, deletion succeeded
                info!(
                    id = %id,
                    model_id = %record.model_id,
                    "Model deleted atomically (was not active)"
                );
                let model = record.try_into()?;
                Ok(Some(model))
            }
            None => {
                // Model was active OR doesn't exist
                // Query again to determine which case
                let exists = self.exists_by_id(id).await?;
                if exists {
                    Err(AppError::InvalidInput(format!(
                        "Cannot delete model with id '{}': it is currently active. Deactivate it first.",
                        id
                    )))
                } else {
                    // Model doesn't exist - idempotent success
                    info!(id = %id, "Model already deleted (idempotent)");
                    Ok(None)
                }
            }
        }
    }

    /// Check if model exists by model_id (helper for error handling)
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model identifier to check
    ///
    /// # Returns
    ///
    /// true if model exists, false otherwise
    async fn exists_by_model_id(&self, model_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models WHERE model_id = ?")
            .bind(model_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!(error = %e, model_id = %model_id, "Failed to check model existence");
                AppError::Database(format!("Failed to check model existence: {}", e))
            })?;

        Ok(count > 0)
    }

    /// Check if model exists by id (primary key) (helper for error handling)
    ///
    /// # Arguments
    ///
    /// * `id` - The unique record ID to check
    ///
    /// # Returns
    ///
    /// true if model exists, false otherwise
    async fn exists_by_id(&self, id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!(error = %e, id = %id, "Failed to check model existence by id");
                AppError::Database(format!("Failed to check model existence: {}", e))
            })?;

        Ok(count > 0)
    }

    /// Delete a downloaded model by ID
    ///
    /// # Arguments
    ///
    /// * `id` - The unique record ID
    ///
    /// # Business Logic
    ///
    /// - Removes the record from database
    /// - Does NOT delete the file from filesystem (caller's responsibility)
    ///
    /// # Returns
    ///
    /// Ok(()) if deleted or if ID doesn't exist
    pub async fn delete_by_id(&self, id: &str) -> Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM models
            WHERE id = ?1
            "#,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, id = %id, "Failed to delete downloaded model");
            AppError::Database(format!("Failed to delete downloaded model: {}", e))
        })?;

        debug!(id = %id, "Downloaded model deleted");
        Ok(())
    }

    /// Check if a model is already downloaded
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model identifier to check
    ///
    /// # Returns
    ///
    /// true if model exists in database, false otherwise
    pub async fn is_downloaded(&self, model_id: &str) -> Result<bool> {
        let model_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM models WHERE model_id = ?1 LIMIT 1",
        )
        .bind(model_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to check model status");
            AppError::Database(format!("Failed to check model status: {}", e))
        })?;

        let Some(status) = model_status else {
            return Ok(false);
        };

        if status != "completed" {
            return Ok(false);
        }

        let rows = sqlx::query(
            r#"
            SELECT file_path, status, size_bytes
            FROM model_files
            WHERE model_id = ?1
            "#,
        )
        .bind(model_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to check model files");
            AppError::Database(format!("Failed to check model files: {}", e))
        })?;

        if rows.is_empty() {
            // Legacy fallback for old rows that may not have model_files populated.
            let model = self.find_by_model_id(model_id).await?;
            return Ok(model.is_some_and(|m| {
                let path = m.file_path();
                path.exists() && path.is_file()
            }));
        }

        for row in rows {
            let file_path: String = row
                .try_get("file_path")
                .map_err(|e| AppError::Database(format!("Invalid model_files.file_path: {}", e)))?;
            let file_status: String = row
                .try_get("status")
                .map_err(|e| AppError::Database(format!("Invalid model_files.status: {}", e)))?;
            let expected_size: i64 = row.try_get("size_bytes").map_err(|e| {
                AppError::Database(format!("Invalid model_files.size_bytes: {}", e))
            })?;

            if file_status != "completed" {
                return Ok(false);
            }

            let path = PathBuf::from(&file_path);
            if !path.exists() || !path.is_file() {
                return Ok(false);
            }

            if expected_size > 0 {
                let actual_size = std::fs::metadata(&path)
                    .map(|m| m.len() as i64)
                    .unwrap_or_default();
                if actual_size < expected_size {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}

// Implement domain trait for dependency inversion
use crate::domain::repositories::downloaded_model_repository::DownloadedModelRepository as DownloadedModelRepositoryTrait;
use async_trait::async_trait;

#[async_trait]
impl DownloadedModelRepositoryTrait for DownloadedModelRepository {
    /// Delete a downloaded model by its model_id
    ///
    /// This implements the domain trait contract by delegating to
    /// a SQL DELETE operation filtered by model_id.
    async fn delete(&self, model_id: &str) -> Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM models
            WHERE model_id = ?1
            "#,
            model_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to delete downloaded model by model_id");
            AppError::Database(format!("Failed to delete downloaded model: {}", e))
        })?;

        debug!(model_id = %model_id, "Downloaded model deleted by model_id");
        Ok(())
    }

    /// Find a downloaded model by its model_id
    ///
    /// This implements the domain trait contract by delegating to
    /// the existing find_by_model_id method.
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<DownloadedModel>> {
        // Delegate to existing method
        self.find_by_model_id(model_id).await
    }

    /// List all downloaded models
    ///
    /// This implements the domain trait contract by delegating to
    /// the existing list_all method.
    async fn list(&self) -> Result<Vec<DownloadedModel>> {
        self.list_all().await
    }

    /// Delete a downloaded model by model_id if it's not the active model
    ///
    /// # Returns
    ///
    /// Ok(true) if deleted, Ok(false) if not deleted (because it's active or doesn't exist)
    async fn delete_by_model_id_if_not_active(
        &self,
        model_id: &str,
        active_model_id: &str,
    ) -> Result<bool> {
        // Atomic DELETE with WHERE guard checking both model id and active flags.
        let result = sqlx::query(
            r#"
            DELETE FROM models
            WHERE model_id = ?1
              AND model_id != ?2
              AND is_active_for_chat = 0
              AND is_active_for_embedding = 0
            "#,
        )
        .bind(model_id)
        .bind(active_model_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to delete model if not active");
            AppError::Database(format!("Failed to delete model: {}", e))
        })?;

        Ok(result.rows_affected() > 0)
    }
}

// === ModelStoragePort Implementation ===
//
// Implements the application port for use cases that need model storage operations.

use crate::application::ports::model_storage::{
    DownloadedModel as DownloadedModelDto, ModelStoragePort,
};

#[async_trait::async_trait]
impl ModelStoragePort for DownloadedModelRepository {
    async fn list_models(&self) -> Result<Vec<DownloadedModelDto>> {
        let models = self.list().await?;

        // Convert domain entities to DTOs
        Ok(models
            .into_iter()
            .map(|model| DownloadedModelDto {
                model_id: model.model_id().to_string(),
                path: model.file_path().to_path_buf(),
                size_bytes: model.file_size_bytes() as u64, // Convert i64 to u64
                downloaded_at: *model.downloaded_at(),
            })
            .collect())
    }

    async fn is_model_downloaded(&self, model_id: &str) -> Result<bool> {
        self.is_downloaded(model_id).await
    }

    async fn get_model_path(&self, model_id: &str) -> Result<std::path::PathBuf> {
        let model = self
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        let path = model.file_path().to_path_buf();
        if !path.exists() || !path.is_file() {
            return Err(AppError::NotFound(format!(
                "Model file not found on disk: {}",
                path.display()
            )));
        }

        Ok(path)
    }

    async fn delete_model(&self, model_id: &str) -> Result<()> {
        // For ModelStoragePort, we use simple delete (without active check)
        // since this is used by use cases that have their own business logic
        self.delete(model_id).await
    }
}

fn build_model_select_query(suffix: &str) -> String {
    format!(
        r#"
{MODEL_FILE_SUMMARY_CTE}
{MODEL_SELECT_BASE}
{suffix}
"#
    )
}

fn parse_optional_db_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    match value {
        Some(v) => parse_db_timestamp(v)
            .map(Some)
            .ok_or_else(|| AppError::InvalidData("Invalid timestamp format".to_string())),
        None => Ok(None),
    }
}

fn parse_db_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
        })
}
