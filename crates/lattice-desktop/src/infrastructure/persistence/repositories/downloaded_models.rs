//! Downloaded Model Repository
//!
//! Manages persistence of downloaded model records in SQLite.

use chrono::Utc;
use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;
use tracing::{debug, error, info};

mod activation;
mod ports;
mod rows;

use rows::{
    build_model_select_query, fallback_artifact_path, location_is_available,
    normalize_model_type_for_models_table, DownloadedModelRecord,
};

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

    /// Save a downloaded model (insert or update).
    ///
    /// # Business Logic
    ///
    /// - Uses UPSERT to handle both insert and update
    /// - Updates all fields on conflict
    /// - If setting is_active_for_chat=1, trigger deactivates other models
    /// - Writes `embedding_artifact_identity` as the model carries it, so a
    ///   freshly constructed model (a re-download) clears the stored identity
    ///   and the next activation hashes the new files
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails
    pub async fn save(&self, model: &DownloadedModel) -> Result<()> {
        // Bind all values to variables to satisfy borrow checker
        let id = model.id().to_string();
        let model_name = model.model_name().to_string();
        let model_id = model.model_id().to_string();
        // Persist the location verbatim — no parent() guessing, no
        // overloaded "is this a file or a dir" semantics. The DB stores
        // exactly what the saga decided.
        let (storage_kind, storage_path) = model.location().to_db();
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
        let is_active_utility = if model.is_active_for_utility() {
            1i64
        } else {
            0i64
        };
        // Legacy `backend` column maps directly off the location variant.
        // Kept in INSERT for compatibility with the column's NOT NULL
        // constraint until a follow-up migration drops it.
        let backend_str = if model.location().is_local() {
            "local"
        } else {
            "ollama"
        };
        let metadata_json = model.metadata_to_json();
        let total_size_bytes = model.file_size_bytes();
        let embedding_artifact_identity = model
            .embedding_artifact_identity()
            .map(|identity| identity.as_str().to_string());

        let mut tx = self.pool.begin().await.map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to begin transaction for model save");
            AppError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        sqlx::query(
            r#"
            INSERT INTO models (
                id, model_name, model_id, storage_kind, storage_path,
                total_size_bytes, status, model_type, architecture,
                downloaded_at, last_used_at, use_count,
                is_active_for_chat, is_active_for_embedding, metadata,
                backend, is_active_for_utility, embedding_artifact_identity
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'completed', ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            ON CONFLICT(model_id) DO UPDATE SET
                model_name = excluded.model_name,
                storage_kind = excluded.storage_kind,
                storage_path = excluded.storage_path,
                total_size_bytes = excluded.total_size_bytes,
                status = excluded.status,
                model_type = excluded.model_type,
                architecture = excluded.architecture,
                downloaded_at = excluded.downloaded_at,
                last_used_at = excluded.last_used_at,
                use_count = excluded.use_count,
                is_active_for_chat = excluded.is_active_for_chat,
                is_active_for_embedding = excluded.is_active_for_embedding,
                metadata = excluded.metadata,
                backend = excluded.backend,
                is_active_for_utility = excluded.is_active_for_utility,
                embedding_artifact_identity = excluded.embedding_artifact_identity
            "#,
        )
        .bind(id)
        .bind(model_name)
        .bind(&model_id)
        .bind(storage_kind)
        .bind(storage_path)
        .bind(total_size_bytes)
        .bind(model_type_str)
        .bind(architecture)
        .bind(downloaded_at)
        .bind(last_used_at)
        .bind(use_count)
        .bind(is_active_chat)
        .bind(is_active_embedding)
        .bind(metadata_json)
        .bind(backend_str)
        .bind(is_active_utility)
        .bind(embedding_artifact_identity)
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
            // Synthesize a single completed model_file row so the
            // download manifest is always non-empty for local models.
            // Use the loadable path's file name (or, for directory
            // layouts, a synthetic stem derived from model_id).
            let fallback_artifact = fallback_artifact_path(model.location());
            let synthetic_file_path = fallback_artifact
                .as_deref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let file_name = fallback_artifact
                .as_deref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}-model", model_id));
            let fallback_file_size = fallback_artifact
                .as_deref()
                .and_then(|path| std::fs::metadata(path).ok())
                .map(|metadata| metadata.len() as i64)
                .unwrap_or(file_size_bytes);
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
            .bind(&synthetic_file_path)
            .bind(&file_name)
            .bind(fallback_file_size)
            .bind(fallback_file_size)
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

    /// List all downloaded models, ordered by download date (newest first).
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

        // A completed record can outlive its files (for example, after moving
        // to another computer). Use the same verification as model loading so
        // settings and the catalog never present missing files as ready.
        let mut models = Vec::new();
        for row in records {
            let model: DownloadedModel = match row.try_into() {
                Ok(model) => model,
                Err(e) => {
                    error!(error = %e, "Failed to convert downloaded model record");
                    continue;
                }
            };
            if self.is_downloaded(model.model_id()).await? {
                models.push(model);
            }
        }

        Ok(models)
    }

    /// Atomically deletes a model ONLY if it is NOT active.
    ///
    /// Fixes CWE-367 (TOCTOU race condition) by using a single atomic DELETE query
    /// with WHERE guard (`is_active_for_chat = 0 AND is_active_for_embedding = 0`).
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
              storage_kind,
              storage_path,
              total_size_bytes AS file_size_bytes,
              model_type,
              architecture,
              downloaded_at,
              last_used_at,
              use_count,
              is_active_for_chat,
              is_active_for_embedding,
              metadata,
              is_active_for_utility,
              embedding_artifact_identity
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

    /// Check if model exists by id (primary key) (helper for error handling)
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

    /// Delete a downloaded model by ID.
    ///
    /// Removes the record from database (does NOT delete files from filesystem).
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

        let Some(model) = self.find_by_model_id(model_id).await? else {
            return Ok(false);
        };
        if !location_is_available(model.location()) {
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
            // Remote-hosted models are always "downloaded" (they're not on disk
            // by definition); local rows are checked against their loadable
            // path, which is a file for GGUF and a directory for safetensors.
            return Ok(true);
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
            if path.is_dir() && matches!(model.location(), ModelLocation::LocalDirectory { .. }) {
                continue;
            }
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests;
