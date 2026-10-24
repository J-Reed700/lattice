//! Reading and switching the active chat, embedding and utility models.

use tracing::{error, info, warn};

use lattice_core::domain::models::downloaded::DownloadedModel;
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::{AppError, Result};

use super::rows::{build_model_select_query, DownloadedModelRecord};
use super::DownloadedModelRepository;

impl DownloadedModelRepository {
    /// Get the currently active chat model.
    ///
    /// Database trigger ensures only one model has is_active_for_chat=1.
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

    /// Get the currently active embedding model.
    ///
    /// Database trigger ensures only one model has is_active_for_embedding=1.
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

    /// Set a model as the active chat model.
    ///
    /// Sets is_active_for_chat=1 for the specified model.
    /// Database trigger automatically sets is_active_for_chat=0 for all others.
    ///
    /// # Errors
    /// Returns NotFound error if model_id doesn't exist.
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

    /// Activate `model_id` for embedding. `identity` is required for local
    /// models; `None` is legal only for remote locations. The guard is in the
    /// statement itself so the invariant holds even under concurrent callers.
    ///
    /// A `Some` identity is recorded on the row; `None` keeps whatever the row
    /// already carries. The database trigger deactivates every other model.
    ///
    /// # Errors
    ///
    /// `NotFound` if `model_id` doesn't exist; `InvalidState` if it is a local
    /// model and neither `identity` nor the row carries an identity.
    pub async fn set_active_embedding_model(
        &self,
        model_id: &str,
        identity: Option<&ArtifactIdentity>,
    ) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE models
            SET is_active_for_embedding = 1,
                embedding_artifact_identity = COALESCE(?2, embedding_artifact_identity),
                updated_at = CURRENT_TIMESTAMP
            WHERE model_id = ?1
              AND (storage_kind = 'remote_ollama'
                   OR COALESCE(?2, embedding_artifact_identity) IS NOT NULL)
            "#,
        )
        .bind(model_id)
        .bind(identity.map(ArtifactIdentity::as_str))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to set active embedding model");
            AppError::Database(format!("Failed to set active embedding model: {}", e))
        })?;

        if result.rows_affected() == 0 {
            if self.find_by_model_id(model_id).await?.is_none() {
                return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
            }
            return Err(AppError::InvalidState(format!(
                "Model '{}' has no recorded artifact identity and cannot be activated for embedding",
                model_id
            )));
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

    /// Clear the active embedding model (set all to inactive). The artifact
    /// identity stays on the row: it describes the files, not the activation.
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

    /// Get the currently active utility model.
    ///
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
}
