use crate::shared::result::Result;
use async_trait::async_trait;

use crate::application::ports::{
    BatchJobRepositoryPort, ChunkRepositoryPort, DocumentRepositoryPort, EmbeddingRepositoryPort,
};

use super::{SearchRepository, SystemRepository};

#[async_trait]
pub trait ModelRepositoryPort: Send + Sync {
    async fn find_by_model_id(
        &self,
        model_id: &str,
    ) -> Result<Option<crate::domain::entities::model::Model>>;
    async fn create_model_with_files(
        &self,
        model: &crate::domain::entities::model::Model,
        files: Vec<crate::domain::entities::model_file::ModelFile>,
    ) -> Result<()>;
    async fn update_status_completed(&self, model_id: &str) -> Result<()>;
    async fn update_status_failed(&self, model_id: &str) -> Result<()>;
}

#[async_trait]
pub trait ModelFileRepositoryPort: Send + Sync {
    async fn create(
        &self,
        model_file: &crate::domain::entities::model_file::ModelFile,
    ) -> Result<()>;
    async fn find_by_model_id(
        &self,
        model_id: &str,
    ) -> Result<Vec<crate::domain::entities::model_file::ModelFile>>;
    async fn count_total_files(&self, model_id: &str) -> Result<usize>;
    async fn count_completed_files(&self, model_id: &str) -> Result<usize>;
    async fn update_file_status_by_model_and_name(
        &self,
        model_id: &str,
        file_name: &str,
        status: crate::domain::value_objects::model_status::FileStatus,
        size_bytes: Option<i64>,
    ) -> Result<()>;
}

#[async_trait]
pub trait UnitOfWork: Send + Sync {
    fn chunk_repository(&self) -> Result<Box<dyn ChunkRepositoryPort + Send + '_>>;

    fn document_repository(&self) -> Result<Box<dyn DocumentRepositoryPort + Send + '_>>;

    fn embedding_repository(&self) -> Result<Box<dyn EmbeddingRepositoryPort + Send + '_>>;

    fn search_repository(&self) -> Result<Box<dyn SearchRepository + Send + '_>>;

    fn batch_job_repository(&self) -> Result<Box<dyn BatchJobRepositoryPort + Send + '_>>;

    fn system_repository(&self) -> Result<Box<dyn SystemRepository + Send + '_>>;

    fn model_repository(&self) -> Result<Box<dyn ModelRepositoryPort + '_>>;

    fn model_file_repository(&self) -> Result<Box<dyn ModelFileRepositoryPort + '_>>;

    async fn commit(&mut self) -> Result<()>;

    async fn rollback(&mut self) -> Result<()>;
}

#[async_trait]
pub trait UnitOfWorkFactory: Send + Sync {
    async fn create(&self) -> Result<Box<dyn UnitOfWork + Send>>;
}
