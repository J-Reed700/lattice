use crate::domain::entities::model::Model;
use crate::domain::entities::model_file::ModelFile;
use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};
use crate::error::AppError;
use async_trait::async_trait;

#[async_trait]
pub trait ModelRepository: Send + Sync {
    async fn create(&self, model: &Model) -> Result<(), AppError>;
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<Model>, AppError>;
    async fn find_all(&self) -> Result<Vec<Model>, AppError>;
    async fn update_status(&self, model_id: &str, status: ModelStatus) -> Result<(), AppError>;
    async fn update_file_progress(
        &self,
        model_id: &str,
        file_name: &str,
        downloaded_bytes: i64,
        status: FileStatus,
    ) -> Result<(), AppError>;
    async fn delete(&self, model_id: &str) -> Result<(), AppError>;
}
