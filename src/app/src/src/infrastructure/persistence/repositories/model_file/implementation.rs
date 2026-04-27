use crate::domain::entities::model_file::ModelFile;
use crate::domain::repositories::unit_of_work::ModelFileRepositoryPort;
use crate::domain::value_objects::model_status::FileStatus;
use crate::shared::error::Result;
use async_trait::async_trait;
use sqlx::SqlitePool;

use super::ops;

#[derive(Clone)]
pub struct SqliteModelFileRepository {
    pool: SqlitePool,
}

impl SqliteModelFileRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, model_file: &ModelFile) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::create(&mut conn, model_file).await
    }

    pub async fn find_by_id(&self, file_id: &str) -> Result<Option<ModelFile>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_id(&mut conn, file_id).await
    }

    pub async fn find_by_model_id(&self, model_id: &str) -> Result<Vec<ModelFile>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_model_id(&mut conn, model_id).await
    }

    pub async fn update_file_status(&self, file_id: &str, status: FileStatus) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::update_file_status(&mut conn, file_id, status).await
    }

    pub async fn count_completed_files(&self, model_id: &str) -> Result<usize> {
        let mut conn = self.pool.acquire().await?;
        ops::count_completed_files(&mut conn, model_id).await
    }

    pub async fn count_total_files(&self, model_id: &str) -> Result<usize> {
        let mut conn = self.pool.acquire().await?;
        ops::count_total_files(&mut conn, model_id).await
    }
}

#[async_trait]
impl ModelFileRepositoryPort for SqliteModelFileRepository {
    async fn create(&self, model_file: &ModelFile) -> Result<()> {
        self.create(model_file).await
    }

    async fn find_by_model_id(&self, model_id: &str) -> Result<Vec<ModelFile>> {
        self.find_by_model_id(model_id).await
    }

    async fn count_total_files(&self, model_id: &str) -> Result<usize> {
        self.count_total_files(model_id).await
    }

    async fn count_completed_files(&self, model_id: &str) -> Result<usize> {
        self.count_completed_files(model_id).await
    }

    async fn update_file_status_by_model_and_name(
        &self,
        model_id: &str,
        file_name: &str,
        status: FileStatus,
        size_bytes: Option<i64>,
    ) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::update_file_status_by_model_and_name(
            &mut conn, model_id, file_name, status, size_bytes,
        )
        .await
    }
}
