use async_trait::async_trait;
use crate::domain::entities::model::Model;
use crate::domain::entities::model_file::ModelFile;
use crate::domain::repositories::model_repository::ModelRepository;
use crate::domain::value_objects::model_status::{ModelStatus, FileStatus};
use crate::error::AppError;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct SqliteModelRepository {
    pool: SqlitePool,
}

impl SqliteModelRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ModelRepository for SqliteModelRepository {
    async fn create(&self, model: &Model) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            error!(error = %e, "Failed to begin transaction");
            AppError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        let is_active_for_chat = if model.is_active_for_chat { 1 } else { 0 };
        let is_active_for_embedding = if model.is_active_for_embedding { 1 } else { 0 };
        let use_count = model.use_count as i64;

        sqlx::query!(
            r#"
            INSERT INTO models (
                id, model_id, name, description, base_path, total_size_bytes,
                architecture, model_type, status,
                is_active_for_chat, is_active_for_embedding,
                use_count, last_used_at, metadata,
                created_at, updated_at, downloaded_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
            model.id,
            model.model_id,
            model.name,
            model.description,
            model.base_path,
            model.total_size_bytes,
            model.architecture,
            model.model_type,
            model.status,
            is_active_for_chat,
            is_active_for_embedding,
            use_count,
            model.last_used_at,
            model.metadata,
            model.created_at,
            model.updated_at,
            model.downloaded_at
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model.model_id, "Failed to insert model");
            AppError::Database(format!("Failed to insert model: {}", e))
        })?;

        for file in &model.files {
            sqlx::query!(
                r#"
                INSERT INTO model_files (
                    id, model_id, file_name, file_path, relative_path, size_bytes,
                    downloaded_bytes, checksum_sha256, download_url, status,
                    created_at, updated_at, downloaded_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
                file.id,
                file.model_id,
                file.file_name,
                file.file_path,
                file.relative_path,
                file.size_bytes,
                file.downloaded_bytes,
                file.checksum_sha256,
                file.download_url,
                file.status,
                file.created_at,
                file.updated_at,
                file.downloaded_at
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                error!(error = %e, file_name = %file.file_name, "Failed to insert model file");
                AppError::Database(format!("Failed to insert model file: {}", e))
            })?;
        }

        tx.commit().await.map_err(|e| {
            error!(error = %e, "Failed to commit transaction");
            AppError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        info!(model_id = %model.model_id, "Model created successfully");
        Ok(())
    }

    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<Model>, AppError> {
        let model_record = sqlx::query!(
            r#"
            SELECT id, model_id, name, description, base_path, total_size_bytes,
                   architecture, model_type as "model_type: String",
                   status as "status: ModelStatus",
                   is_active_for_chat, is_active_for_embedding,
                   use_count as "use_count: i64",
                   last_used_at as "last_used_at: Option<DateTime<Utc>>",
                   metadata as "metadata: Option<serde_json::Value>",
                   created_at as "created_at: DateTime<Utc>",
                   updated_at as "updated_at: DateTime<Utc>",
                   downloaded_at as "downloaded_at: Option<DateTime<Utc>>"
            FROM models
            WHERE model_id = ?1
            "#,
            model_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to find model");
            AppError::Database(format!("Failed to find model: {}", e))
        })?;

        match model_record {
            Some(row) => {
                let files = sqlx::query!(
                    r#"
                    SELECT id, model_id, file_name, file_path, relative_path, size_bytes,
                           downloaded_bytes, checksum_sha256, download_url,
                           status as "status: FileStatus",
                           created_at as "created_at: DateTime<Utc>",
                           updated_at as "updated_at: DateTime<Utc>",
                           downloaded_at as "downloaded_at: Option<DateTime<Utc>>"
                    FROM model_files
                    WHERE model_id = ?1
                    ORDER BY file_name
                    "#,
                    model_id
                )
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    error!(error = %e, model_id = %model_id, "Failed to fetch model files");
                    AppError::Database(format!("Failed to fetch model files: {}", e))
                })?
                .into_iter()
                .map(|file_row| ModelFile {
                    id: file_row.id,
                    model_id: file_row.model_id,
                    file_name: file_row.file_name,
                    file_path: file_row.file_path,
                    relative_path: file_row.relative_path,
                    size_bytes: file_row.size_bytes,
                    downloaded_bytes: file_row.downloaded_bytes,
                    checksum_sha256: file_row.checksum_sha256,
                    download_url: file_row.download_url,
                    status: file_row.status,
                    created_at: file_row.created_at,
                    updated_at: file_row.updated_at,
                    downloaded_at: file_row.downloaded_at.flatten(),
                })
                .collect();

                let model = Model {
                    id: row.id,
                    model_id: row.model_id,
                    name: row.name,
                    description: row.description,
                    base_path: row.base_path,
                    total_size_bytes: row.total_size_bytes,
                    architecture: row.architecture,
                    model_type: row.model_type,
                    status: row.status,
                    files,
                    is_active_for_chat: row.is_active_for_chat != 0,
                    is_active_for_embedding: row.is_active_for_embedding != 0,
                    use_count: row.use_count as u64,
                    last_used_at: row.last_used_at.flatten(),
                    metadata: row.metadata.flatten(),
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    downloaded_at: row.downloaded_at.flatten(),
                };

                debug!(model_id = %model_id, file_count = model.files.len(), "Found model with files");
                Ok(Some(model))
            }
            None => {
                debug!(model_id = %model_id, "Model not found");
                Ok(None)
            }
        }
    }

    async fn find_all(&self) -> Result<Vec<Model>, AppError> {
        let model_records = sqlx::query!(
            r#"
            SELECT id, model_id, name, description, base_path, total_size_bytes,
                   architecture, model_type as "model_type: String",
                   status as "status: ModelStatus",
                   is_active_for_chat, is_active_for_embedding,
                   use_count as "use_count: i64",
                   last_used_at as "last_used_at: Option<DateTime<Utc>>",
                   metadata as "metadata: Option<serde_json::Value>",
                   created_at as "created_at: DateTime<Utc>",
                   updated_at as "updated_at: DateTime<Utc>",
                   downloaded_at as "downloaded_at: Option<DateTime<Utc>>"
            FROM models
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to fetch all models");
            AppError::Database(format!("Failed to fetch all models: {}", e))
        })?;

        let mut models = Vec::new();

        for row in model_records {
            let files = sqlx::query!(
                r#"
                SELECT id, model_id, file_name, file_path, relative_path, size_bytes,
                       downloaded_bytes, checksum_sha256, download_url,
                       status as "status: FileStatus",
                       created_at as "created_at: DateTime<Utc>",
                       updated_at as "updated_at: DateTime<Utc>",
                       downloaded_at as "downloaded_at: Option<DateTime<Utc>>"
                FROM model_files
                WHERE model_id = ?1
                ORDER BY file_name
                "#,
                row.model_id
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!(error = %e, model_id = %row.model_id, "Failed to fetch model files");
                AppError::Database(format!("Failed to fetch model files: {}", e))
            })?
            .into_iter()
            .map(|file_row| ModelFile {
                id: file_row.id,
                model_id: file_row.model_id,
                file_name: file_row.file_name,
                file_path: file_row.file_path,
                relative_path: file_row.relative_path,
                size_bytes: file_row.size_bytes,
                downloaded_bytes: file_row.downloaded_bytes,
                checksum_sha256: file_row.checksum_sha256,
                download_url: file_row.download_url,
                status: file_row.status,
                created_at: file_row.created_at,
                updated_at: file_row.updated_at,
                downloaded_at: file_row.downloaded_at.flatten(),
            })
            .collect();

            models.push(Model {
                id: row.id,
                model_id: row.model_id,
                name: row.name,
                description: row.description,
                base_path: row.base_path,
                total_size_bytes: row.total_size_bytes,
                architecture: row.architecture,
                model_type: row.model_type,
                status: row.status,
                files,
                is_active_for_chat: row.is_active_for_chat != 0,
                is_active_for_embedding: row.is_active_for_embedding != 0,
                use_count: row.use_count as u64,
                last_used_at: row.last_used_at.flatten(),
                metadata: row.metadata.flatten(),
                created_at: row.created_at,
                updated_at: row.updated_at,
                downloaded_at: row.downloaded_at.flatten(),
            });
        }

        info!(count = models.len(), "Fetched all models");
        Ok(models)
    }

    async fn update_status(&self, model_id: &str, status: ModelStatus) -> Result<(), AppError> {
        let now = Utc::now();

        sqlx::query!(
            r#"
            UPDATE models
            SET status = ?1, updated_at = ?2
            WHERE model_id = ?3
            "#,
            status,
            now,
            model_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to update model status");
            AppError::Database(format!("Failed to update model status: {}", e))
        })?;

        debug!(model_id = %model_id, ?status, "Model status updated");
        Ok(())
    }

    async fn update_file_progress(
        &self,
        model_id: &str,
        file_name: &str,
        downloaded_bytes: i64,
        status: FileStatus,
    ) -> Result<(), AppError> {
        let now = Utc::now();

        sqlx::query!(
            r#"
            UPDATE model_files
            SET downloaded_bytes = ?1, status = ?2, updated_at = ?3
            WHERE model_id = ?4 AND file_name = ?5
            "#,
            downloaded_bytes,
            status,
            now,
            model_id,
            file_name
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, file_name = %file_name,
                   "Failed to update file progress");
            AppError::Database(format!("Failed to update file progress: {}", e))
        })?;

        debug!(model_id = %model_id, file_name = %file_name, downloaded_bytes,
               ?status, "File progress updated");
        Ok(())
    }

    async fn delete(&self, model_id: &str) -> Result<(), AppError> {
        sqlx::query!(
            r#"
            DELETE FROM models WHERE model_id = ?1
            "#,
            model_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to delete model");
            AppError::Database(format!("Failed to delete model: {}", e))
        })?;

        info!(model_id = %model_id, "Model deleted successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    async fn setup_test_db() -> SqlitePool {
        SqlitePool::connect(":memory:").await.unwrap()
    }

    #[tokio::test]
    async fn test_create_and_find_model() {
        let pool = setup_test_db().await;
        let repo = SqliteModelRepository::new(pool.clone());

        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let model = Model {
            id: "test-1".to_string(),
            model_id: "llama-2-7b".to_string(),
            name: "Llama 2 7B".to_string(),
            description: Some("Test model".to_string()),
            base_path: "/models/llama-2-7b".to_string(),
            total_size_bytes: 1024,
            architecture: "LLAMA".to_string(),
            model_type: "chat".to_string(),
            status: ModelStatus::Pending,
            files: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            downloaded_at: None,
        };

        repo.create(&model).await.unwrap();
        let found = repo.find_by_model_id("llama-2-7b").await.unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Llama 2 7B");
    }
}
