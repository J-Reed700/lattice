use crate::domain::entities::model::Model;
use crate::domain::entities::model_file::ModelFile;
use crate::domain::repositories::unit_of_work::ModelRepositoryPort;
use crate::domain::value_objects::model_status::ModelStatus;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::SqlitePool;
use tracing::{debug, error};

use super::ops;

#[derive(Clone)]
pub struct SqliteModelRepository {
    pool: SqlitePool,
}

impl SqliteModelRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_by_model_id(&self, model_id: &str) -> Result<Option<Model>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_model_id(&mut conn, model_id).await
    }

    pub async fn find_model_with_files(
        &self,
        model_id: &str,
    ) -> Result<Option<(Model, Vec<ModelFile>)>> {
        let model = self.find_by_model_id(model_id).await?;

        match model {
            Some(m) => {
                let mut conn = self.pool.acquire().await?;
                let files = ops::get_model_files(&mut conn, model_id).await?;
                Ok(Some((m, files)))
            }
            None => Ok(None),
        }
    }

    pub async fn create_model_with_files(
        &self,
        model: &Model,
        files: Vec<ModelFile>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            error!(error = %e, "Failed to begin transaction");
            AppError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        ops::upsert_model(&mut tx, model).await?;

        for file in files {
            ops::upsert_model_file(&mut tx, &file).await?;
        }

        tx.commit().await.map_err(|e| {
            error!(error = %e, "Failed to commit transaction");
            AppError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        debug!(model_id = %model.model_id, "Model with files created/updated successfully");
        Ok(())
    }

    pub async fn update_model_status(&self, model_id: &str, status: ModelStatus) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::update_model_status(&mut conn, model_id, status).await
    }

    pub async fn set_active_for_chat(&self, model_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            error!(error = %e, "Failed to begin transaction");
            AppError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        ops::deactivate_all_chat_models(&mut tx).await?;
        ops::activate_chat_model(&mut tx, model_id).await?;

        tx.commit().await.map_err(|e| {
            error!(error = %e, "Failed to commit transaction");
            AppError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        Ok(())
    }

    pub async fn set_active_for_embedding(&self, model_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            error!(error = %e, "Failed to begin transaction");
            AppError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        ops::deactivate_all_embedding_models(&mut tx).await?;
        ops::activate_embedding_model(&mut tx, model_id).await?;

        tx.commit().await.map_err(|e| {
            error!(error = %e, "Failed to commit transaction");
            AppError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        Ok(())
    }

    pub async fn list_all(&self) -> Result<Vec<Model>> {
        let mut conn = self.pool.acquire().await?;
        ops::list_all(&mut conn).await
    }

    pub async fn get_active_chat_model(&self) -> Result<Option<Model>> {
        let mut conn = self.pool.acquire().await?;
        ops::get_active_chat_model(&mut conn).await
    }

    pub async fn get_active_embedding_model(&self) -> Result<Option<Model>> {
        let mut conn = self.pool.acquire().await?;
        ops::get_active_embedding_model(&mut conn).await
    }

    pub async fn save(&self, model: &Model) -> Result<()> {
        self.create_model_with_files(model, vec![]).await
    }

    pub async fn is_downloaded(&self, model_id: &str) -> Result<bool> {
        let model_opt = self.find_by_model_id(model_id).await?;
        Ok(model_opt.is_some())
    }

    pub async fn set_active_chat_model(&self, model_id: &str) -> Result<()> {
        self.set_active_for_chat(model_id).await
    }

    pub async fn set_active_embedding_model(&self, model_id: &str) -> Result<()> {
        self.set_active_for_embedding(model_id).await
    }

    pub async fn clear_active_chat_model(&self) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::clear_active_chat_model(&mut conn).await
    }

    pub async fn clear_active_embedding_model(&self) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::clear_active_embedding_model(&mut conn).await
    }

    pub async fn delete_if_not_active(&self, model_id: &str) -> Result<Option<Model>> {
        let model_opt = self.find_by_model_id(model_id).await?;

        match model_opt {
            Some(model) => {
                if model.is_active_for_chat || model.is_active_for_embedding {
                    return Err(AppError::InvalidInput(format!(
                        "Cannot delete model '{}': it is currently active. Deactivate it first.",
                        model_id
                    )));
                }

                let mut conn = self.pool.acquire().await?;
                ops::delete_if_not_active(&mut conn, model_id).await?;
                Ok(Some(model))
            }
            None => Ok(None),
        }
    }

    pub async fn update_status_completed(&self, model_id: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::update_status_completed(&mut conn, model_id).await
    }

    pub async fn update_status_failed(&self, model_id: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::update_status_failed(&mut conn, model_id).await
    }
}

#[async_trait]
impl ModelRepositoryPort for SqliteModelRepository {
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<Model>> {
        self.find_by_model_id(model_id).await
    }

    async fn create_model_with_files(&self, model: &Model, files: Vec<ModelFile>) -> Result<()> {
        self.create_model_with_files(model, files).await
    }

    async fn update_status_completed(&self, model_id: &str) -> Result<()> {
        self.update_status_completed(model_id).await
    }

    async fn update_status_failed(&self, model_id: &str) -> Result<()> {
        self.update_status_failed(model_id).await
    }
}
