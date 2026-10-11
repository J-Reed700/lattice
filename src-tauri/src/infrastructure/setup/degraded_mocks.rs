use crate::features::web::{WebIngestionResult, WebIngestionServiceTrait};
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

pub fn create_degraded_web_ingestion() -> Arc<dyn WebIngestionServiceTrait> {
    Arc::new(DegradedWebIngestionService)
}
