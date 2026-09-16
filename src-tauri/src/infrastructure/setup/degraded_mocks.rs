use crate::application::ports::batch_job_repository_port::BatchJobStatus;
use crate::domain::value_objects::file_metadata::FileMetadata;
use crate::features::batch::{BatchFileImportServiceTrait, BatchUrlImportServiceTrait};
use crate::features::web::{WebIngestionResult, WebIngestionServiceTrait};
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;

const AI_MODELS_NOT_INSTALLED_MSG: &str =
    "AI models are not installed. This feature requires embedding models. \
     Please download models from Settings → Models to enable this functionality.";

pub struct DegradedWebIngestionService;

#[async_trait::async_trait]
impl WebIngestionServiceTrait for DegradedWebIngestionService {
    async fn ingest_url(&self, _url: &str) -> Result<WebIngestionResult> {
        Err(AppError::AiModelsNotInstalled(
            AI_MODELS_NOT_INSTALLED_MSG.to_string(),
        ))
    }
}

pub struct DegradedBatchFileImportService;

#[async_trait::async_trait]
impl BatchFileImportServiceTrait for DegradedBatchFileImportService {
    async fn start_batch_import(&self, _file_paths: Vec<ValidatedFilePath>) -> Result<String> {
        Err(AppError::AiModelsNotInstalled(
            AI_MODELS_NOT_INSTALLED_MSG.to_string(),
        ))
    }

    fn validate_file(&self, _path: &ValidatedFilePath) -> Result<FileMetadata> {
        Err(AppError::AiModelsNotInstalled(
            AI_MODELS_NOT_INSTALLED_MSG.to_string(),
        ))
    }
}

pub struct DegradedBatchUrlImportService;

#[async_trait::async_trait]
impl BatchUrlImportServiceTrait for DegradedBatchUrlImportService {
    async fn start_batch_import(
        &self,
        _urls: Vec<String>,
        _options: Option<String>,
    ) -> Result<String> {
        Err(AppError::AiModelsNotInstalled(
            AI_MODELS_NOT_INSTALLED_MSG.to_string(),
        ))
    }

    async fn get_batch_status(&self, _job_id: &str) -> Result<BatchJobStatus> {
        Err(AppError::AiModelsNotInstalled(
            AI_MODELS_NOT_INSTALLED_MSG.to_string(),
        ))
    }

    async fn cancel_batch_job(&self, _job_id: &str) -> Result<usize> {
        Err(AppError::AiModelsNotInstalled(
            AI_MODELS_NOT_INSTALLED_MSG.to_string(),
        ))
    }
}

pub fn create_degraded_web_ingestion() -> Arc<dyn WebIngestionServiceTrait> {
    Arc::new(DegradedWebIngestionService)
}

pub fn create_degraded_batch_file_import() -> Arc<dyn BatchFileImportServiceTrait> {
    Arc::new(DegradedBatchFileImportService)
}

pub fn create_degraded_batch_url_import() -> Arc<dyn BatchUrlImportServiceTrait> {
    Arc::new(DegradedBatchUrlImportService)
}
