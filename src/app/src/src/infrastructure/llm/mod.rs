//! LLM integration module.
//!
//! Provides clients for local LLM inference including llama.cpp and Ollama.
//! This module supports both HTTP-based clients and local GPU-accelerated inference.

// Single-file modules grouped under modules/ for filesystem organization.
#[path = "modules/circuit_breaker.rs"]
pub mod circuit_breaker;
#[path = "modules/factory.rs"]
pub mod factory;
#[path = "modules/local_client.rs"]
pub mod local_client;
#[path = "modules/model_catalog_adapter.rs"]
pub mod model_catalog_adapter;
#[path = "modules/model_storage_adapter.rs"]
pub mod model_storage_adapter;
#[path = "modules/models.rs"]
pub mod models;
#[path = "modules/noop_client.rs"]
pub mod noop_client;
#[path = "modules/ollama_client.rs"]
pub mod ollama_client;
#[path = "modules/traits.rs"]
pub mod traits;
#[path = "modules/types.rs"]
pub mod types;

// Directory-backed modules.
pub mod inference;
pub mod system;

pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
pub use factory::{
    create_llm, create_llm_with_fallback, find_local_model, is_ollama_available, LLMConfig,
};
pub use inference::{InferenceConfig, InferenceEngine, ModelLoader};
pub use local_client::LocalLLMClient;
pub use model_catalog_adapter::HardcodedModelCatalog;
pub use model_storage_adapter::FilesystemModelStorage;
pub use models::{
    get_catalog, HardwareCapabilities, ModelCatalog, ModelFamily, ModelInfo, ModelRecommender,
    PerformanceTier, Quantization, RecommendedModel,
};
pub use noop_client::NoOpLLMClient;
pub use ollama_client::OllamaClient;
pub use system::{detect_capabilities, GPUInfo, GPUVendor, Platform, SystemCapabilities};
pub use traits::{GenerationConfig, LLMClient};
pub use types::*;

// ============================================================================
// Model Download Utilities
// ============================================================================

use crate::application::dtos::llm_dto::{DownloadModelRequestDto, DownloadModelResponseDto};
use crate::application::ports::model_storage::ModelStoragePort;
use crate::application::use_cases::llm::DownloadModelUseCase;
use crate::shared::error::Result;
use std::path::PathBuf;
use std::sync::Arc;

/// Model downloader for managing LLM model files.
#[derive(Clone)]
pub struct ModelDownloader {
    download_use_case: Arc<DownloadModelUseCase>,
    model_storage: Arc<dyn ModelStoragePort>,
}

impl ModelDownloader {
    pub fn new(
        download_use_case: Arc<DownloadModelUseCase>,
        model_storage: Arc<dyn ModelStoragePort>,
    ) -> Self {
        Self {
            download_use_case,
            model_storage,
        }
    }

    pub async fn download(&self, model_id: &str) -> Result<()> {
        self.download_with_response(model_id).await?;
        Ok(())
    }

    pub async fn download_with_response(&self, model_id: &str) -> Result<DownloadModelResponseDto> {
        let request = DownloadModelRequestDto {
            model_id: model_id.to_string(),
        };
        self.download_use_case.execute(request).await
    }

    pub async fn is_downloaded(&self, model: &ModelInfo) -> Result<bool> {
        self.model_storage.is_model_downloaded(&model.name).await
    }

    pub async fn model_path(&self, model: &ModelInfo) -> Result<PathBuf> {
        self.model_storage.get_model_path(&model.name).await
    }

    pub async fn delete_model(&self, model_id: &str) -> Result<()> {
        self.model_storage.delete_model(model_id).await
    }

    pub async fn list_downloaded(&self) -> Result<Vec<String>> {
        let models = self.model_storage.list_models().await?;
        Ok(models.into_iter().map(|model| model.model_id).collect())
    }
}

/// Download progress tracking.
pub type DownloadProgress = crate::domain::DownloadProgress;
