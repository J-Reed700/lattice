//! Downloaded Model Repository
//!
//! Manages persistence of downloaded model records in SQLite.

use crate::domain::downloaded_model::{DownloadedModel, ModelLocation, ModelType};
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

/// Database record structure for RETURNING queries.
///
/// Mirrors the row produced by `MODEL_SELECT_BASE`. The `(storage_kind,
/// storage_path)` pair is the canonical representation of `ModelLocation`
/// in the DB; both are read verbatim with no JOIN-time recomputation.
#[derive(Debug, sqlx::FromRow)]
struct DownloadedModelRecord {
    id: String,
    model_name: String,
    model_id: String,
    storage_kind: String,
    storage_path: Option<String>,
    file_size_bytes: i64,
    model_type: String,
    architecture: String,
    downloaded_at: Option<String>,
    last_used_at: Option<String>,
    use_count: i64,
    is_active_for_chat: i64,
    is_active_for_embedding: i64,
    metadata: Option<String>,
    is_active_for_utility: i64,
}

/// Read columns directly from `models`. The previous version of this
/// query had a CTE that re-derived the "primary file" by alphabetical
/// order over `model_files` and `COALESCE`-overrode the saga-written
/// path, which silently corrupted multi-file safetensors models. The
/// SSOT for the loadable artifact is `models.storage_kind` +
/// `models.storage_path`, written by the saga and never recomputed.
const MODEL_SELECT_BASE: &str = r#"
SELECT
    m.id,
    m.model_name,
    m.model_id,
    m.storage_kind,
    m.storage_path,
    COALESCE(m.total_size_bytes, 0) AS file_size_bytes,
    m.model_type,
    m.architecture,
    m.downloaded_at,
    m.last_used_at,
    m.use_count,
    m.is_active_for_chat,
    m.is_active_for_embedding,
    m.metadata,
    m.is_active_for_utility
FROM models m
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
        ModelType::Transcription => "custom",
    }
}

fn directory_has_loadable_weights(path: &Path) -> bool {
    path.is_dir()
        && std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .any(|entry| {
                entry.is_file()
                    && entry.extension().and_then(|value| value.to_str()) == Some("safetensors")
            })
}

fn location_is_available(location: &ModelLocation) -> bool {
    match location {
        ModelLocation::LocalFile { path } => path.is_file(),
        ModelLocation::LocalDirectory { path } => directory_has_loadable_weights(path),
        ModelLocation::RemoteOllama => true,
    }
}

fn fallback_artifact_path(location: &ModelLocation) -> Option<PathBuf> {
    match location {
        ModelLocation::LocalFile { path } => Some(path.clone()),
        ModelLocation::LocalDirectory { path } => Some(path.join("model.safetensors")),
        ModelLocation::RemoteOllama => None,
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
        let metadata_val = match DownloadedModel::metadata_from_json(record.metadata.as_deref()) {
            Ok(metadata) => metadata,
            Err(e) => {
                warn!(
                    model_id = %record.model_id,
                    error = %e,
                    "Ignoring invalid model metadata JSON"
                );
                None
            }
        };

        // Convert is_active flags from integer to bool
        let is_active_chat = record.is_active_for_chat != 0;
        let is_active_embedding = record.is_active_for_embedding != 0;
        let is_active_utility = record.is_active_for_utility != 0;

        let location =
            ModelLocation::from_db(&record.storage_kind, record.storage_path).map_err(|e| {
                AppError::InvalidData(format!(
                    "Invalid storage_kind/storage_path for model_id '{}': {}",
                    record.model_id, e
                ))
            })?;

        Ok(DownloadedModel::from_db(
            record.id,
            record.model_name,
            record.model_id,
            location,
            record.file_size_bytes,
            model_type_enum,
            record.architecture,
            downloaded_at_dt,
            last_used_at_dt,
            record.use_count,
            is_active_chat,
            is_active_embedding,
            metadata_val,
            is_active_utility,
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
                backend, is_active_for_utility
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'completed', ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
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
                is_active_for_utility = excluded.is_active_for_utility
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
                // Ollama-served models live remotely; there is no local file
                // for `is_downloaded` to verify. Skip the FS check for them.
                if !model.location().is_local() || self.is_downloaded(model.model_id()).await? {
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
                // Ollama-served models live remotely; there is no local file
                // for `is_downloaded` to verify. Skip the FS check for them.
                if !model.location().is_local() || self.is_downloaded(model.model_id()).await? {
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

    /// Set a model as the active utility model
    ///
    /// # Business Logic
    ///
    /// - Sets is_active_for_utility=1 for the specified model
    /// - Database trigger (`ensure_single_active_utility_model`) automatically
    ///   sets is_active_for_utility=0 for all other rows
    /// - Fails if model_id doesn't exist
    pub async fn set_active_utility_model(&self, model_id: &str) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE models
            SET is_active_for_utility = 1
            WHERE model_id = ?1
            "#,
        )
        .bind(model_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to set active utility model");
            AppError::Database(format!("Failed to set active utility model: {}", e))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
        }

        info!(model_id = %model_id, "Set active utility model");
        Ok(())
    }

    /// Clear the active utility model (set all to inactive)
    pub async fn clear_active_utility_model(&self) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE models
            SET is_active_for_utility = 0
            WHERE is_active_for_utility = 1
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to clear active utility model");
            AppError::Database(format!("Failed to clear active utility model: {}", e))
        })?;

        info!("Active utility model cleared");
        Ok(())
    }

    /// Get the currently active utility model
    ///
    /// # Returns
    ///
    /// Some(DownloadedModel) if an active utility model exists, None otherwise.
    /// For Ollama-backed rows the local-file `is_downloaded` check is skipped
    /// because the model lives on a remote server.
    pub async fn get_active_utility_model(&self) -> Result<Option<DownloadedModel>> {
        let query = build_model_select_query("WHERE m.is_active_for_utility = 1 LIMIT 1");
        let record = sqlx::query_as::<_, DownloadedModelRecord>(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to get active utility model");
                AppError::Database(format!("Failed to get active utility model: {}", e))
            })?;

        match record {
            Some(row) => {
                let model: DownloadedModel = row.try_into()?;
                if !model.location().is_local() || self.is_downloaded(model.model_id()).await? {
                    Ok(Some(model))
                } else {
                    warn!(
                        model_id = %model.model_id(),
                        "Active utility model is not fully downloaded; ignoring active selection"
                    );
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Whether any embedding model is registered and downloaded.
    ///
    /// Used by first-run setup to decide whether the embedding pipeline can
    /// initialize without prompting the user to pick a model.
    pub async fn has_any_embedding_model(&self) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM models
                WHERE model_type = 'embedding'
                  AND status = 'completed'
            )
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to check for any embedding model");
            AppError::Database(format!("Failed to check for embedding models: {}", e))
        })?;

        Ok(exists != 0)
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
              is_active_for_utility
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

        // Convert domain entities to DTOs. Remote (Ollama) entries have
        // no on-disk path; surface an empty PathBuf so the DTO contract
        // (path: PathBuf) stays unchanged for now.
        Ok(models
            .into_iter()
            .map(|model| DownloadedModelDto {
                model_id: model.model_id().to_string(),
                path: model
                    .loadable_path()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default(),
                size_bytes: model.file_size_bytes() as u64,
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

        let path = model
            .loadable_path()
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "Model {} has no local path (remote-hosted)",
                    model_id
                ))
            })?
            .to_path_buf();
        // Either a file (GGUF, single-shard) or a directory (safetensors layout).
        if !path.exists() {
            return Err(AppError::NotFound(format!(
                "Model artifact not found on disk: {}",
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
{MODEL_SELECT_BASE}
{suffix}
"#
    )
}

fn parse_optional_db_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    let Some(v) = value else {
        return Ok(None);
    };

    let trimmed = v.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if let Some(parsed) = parse_db_timestamp(trimmed) {
        return Ok(Some(parsed));
    }

    warn!(timestamp = %trimmed, "Ignoring invalid timestamp format in downloaded model row");
    Ok(None)
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[test]
    fn parse_optional_timestamp_accepts_sqlite_format() {
        let parsed = parse_optional_db_timestamp(Some("2026-03-04 19:53:19"))
            .expect("expected parse to succeed");
        assert!(parsed.is_some());
    }

    #[test]
    fn parse_optional_timestamp_treats_empty_as_none() {
        let parsed = parse_optional_db_timestamp(Some("   ")).expect("expected parse to succeed");
        assert!(parsed.is_none());
    }

    #[test]
    fn parse_optional_timestamp_treats_invalid_as_none() {
        let parsed =
            parse_optional_db_timestamp(Some("not-a-timestamp")).expect("expected no error");
        assert!(parsed.is_none());
    }

    async fn setup_repo() -> DownloadedModelRepository {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        DownloadedModelRepository::new(pool)
    }

    fn make_local_chat_model(model_id: &str) -> DownloadedModel {
        DownloadedModel::new(
            uuid::Uuid::new_v4().to_string(),
            format!("Test Model {}", model_id),
            model_id.to_string(),
            ModelLocation::LocalFile {
                path: std::path::PathBuf::from(format!("/tmp/{}.gguf", model_id)),
            },
            1024,
            "llama".to_string(),
            None,
        )
        .expect("create local chat model")
    }

    fn make_local_embedding_model(model_id: &str) -> DownloadedModel {
        DownloadedModel::new(
            uuid::Uuid::new_v4().to_string(),
            format!("Embed {}", model_id),
            model_id.to_string(),
            ModelLocation::LocalDirectory {
                path: std::path::PathBuf::from(format!("/tmp/{}", model_id)),
            },
            512,
            "bge".to_string(),
            None,
        )
        .expect("create local embedding model")
    }

    #[tokio::test]
    async fn synthetic_ollama_row_loads_via_find_by_model_id() {
        let repo = setup_repo().await;
        let model = repo
            .find_by_model_id("__ollama_server__")
            .await
            .expect("find by model id")
            .expect("synthetic ollama row missing after migrations");

        assert_eq!(model.location(), &ModelLocation::RemoteOllama);
        assert_eq!(model.file_size_bytes(), 0);
        assert_eq!(model.model_id(), "__ollama_server__");
    }

    #[tokio::test]
    async fn set_and_get_active_utility_model_roundtrip() {
        let repo = setup_repo().await;

        // Activate the synthetic ollama row for utility (no FS check).
        repo.set_active_utility_model("__ollama_server__")
            .await
            .expect("set active utility");

        let active = repo
            .get_active_utility_model()
            .await
            .expect("get active utility");
        let active = active.expect("expected active utility model");
        assert_eq!(active.model_id(), "__ollama_server__");
        assert!(active.is_active_for_utility());

        repo.clear_active_utility_model()
            .await
            .expect("clear active utility");
        let after_clear = repo
            .get_active_utility_model()
            .await
            .expect("get active utility post-clear");
        assert!(after_clear.is_none());
    }

    #[tokio::test]
    async fn get_active_chat_model_returns_ollama_row_without_filesystem_check() {
        let repo = setup_repo().await;

        repo.set_active_chat_model("__ollama_server__")
            .await
            .expect("set active chat to ollama");

        let active = repo
            .get_active_chat_model()
            .await
            .expect("get active chat model");
        let active = active.expect("expected ollama row to be returned despite no local file");
        assert_eq!(active.location(), &ModelLocation::RemoteOllama);
        assert_eq!(active.model_id(), "__ollama_server__");
    }

    #[tokio::test]
    async fn has_any_embedding_model_false_on_empty_then_true_after_save() {
        let repo = setup_repo().await;

        // Synthetic ollama row is model_type='chat', so initially no embedding rows.
        let before = repo
            .has_any_embedding_model()
            .await
            .expect("check embedding presence pre-save");
        assert!(!before, "no embedding model expected before save");

        let embedding = make_local_embedding_model("test-embed-1");
        repo.save(&embedding).await.expect("save embedding model");

        let after = repo
            .has_any_embedding_model()
            .await
            .expect("check embedding presence post-save");
        assert!(after, "embedding model should be detected after save");
    }

    #[tokio::test]
    async fn corrective_migration_purges_any_local_embedding_without_safetensors() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory pool");
        sqlx::raw_sql(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE models (
                model_id TEXT PRIMARY KEY,
                model_type TEXT NOT NULL,
                backend TEXT NOT NULL
            );
            CREATE TABLE model_files (
                model_id TEXT NOT NULL,
                file_name TEXT NOT NULL,
                FOREIGN KEY (model_id) REFERENCES models(model_id) ON DELETE CASCADE
            );

            INSERT INTO models VALUES ('third-legacy-onnx', 'embedding', 'local');
            INSERT INTO model_files VALUES ('third-legacy-onnx', 'config.json');
            INSERT INTO model_files VALUES ('third-legacy-onnx', 'tokenizer.json');

            INSERT INTO models VALUES ('valid-candle', 'embedding', 'local');
            INSERT INTO model_files VALUES ('valid-candle', 'model.safetensors');

            INSERT INTO models VALUES ('remote', 'embedding', 'ollama');
            INSERT INTO models VALUES ('chat-without-safetensors', 'chat', 'local');
            "#,
        )
        .execute(&pool)
        .await
        .expect("seed legacy rows");

        sqlx::raw_sql(include_str!(
            "../../../migrations/20260801010000_purge_unloadable_embedding_models.sql"
        ))
        .execute(&pool)
        .await
        .expect("run corrective purge");

        let remaining: Vec<String> =
            sqlx::query_scalar("SELECT model_id FROM models ORDER BY model_id")
                .fetch_all(&pool)
                .await
                .expect("list remaining models");
        assert_eq!(
            remaining,
            vec![
                "chat-without-safetensors".to_string(),
                "remote".to_string(),
                "valid-candle".to_string(),
            ]
        );

        let orphan_children: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM model_files WHERE model_id = 'third-legacy-onnx'",
        )
        .fetch_one(&pool)
        .await
        .expect("count cascaded children");
        assert_eq!(orphan_children, 0);
    }

    #[tokio::test]
    async fn save_and_load_local_model_preserves_location() {
        let repo = setup_repo().await;
        let model = make_local_chat_model("local-chat-1");
        repo.save(&model).await.expect("save local chat model");

        let loaded = repo
            .find_by_model_id("local-chat-1")
            .await
            .expect("find local model")
            .expect("local model missing after save");
        assert!(loaded.location().is_local());
        assert!(matches!(loaded.location(), ModelLocation::LocalFile { .. }));
        assert!(!loaded.is_active_for_utility());
    }

    #[tokio::test]
    async fn directory_model_fallback_tracks_required_weights_file() {
        let repo = setup_repo().await;
        let directory = tempfile::tempdir().expect("create model directory");
        std::fs::write(directory.path().join("model.safetensors"), b"weights")
            .expect("write model weights");

        let model = DownloadedModel::new(
            uuid::Uuid::new_v4().to_string(),
            "Directory Model".to_string(),
            "directory-model".to_string(),
            ModelLocation::LocalDirectory {
                path: directory.path().to_path_buf(),
            },
            7,
            "bge".to_string(),
            None,
        )
        .expect("create directory model");

        repo.save(&model).await.expect("save directory model");
        assert!(repo
            .is_downloaded("directory-model")
            .await
            .expect("check directory model"));

        let file_path: String = sqlx::query_scalar(
            "SELECT file_path FROM model_files WHERE model_id = 'directory-model'",
        )
        .fetch_one(&repo.pool)
        .await
        .expect("load fallback file path");
        assert!(file_path.ends_with("model.safetensors"));
    }
}
