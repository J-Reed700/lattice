//! LLM integration module.
//!
//! Provides clients for local LLM inference including llama.cpp and Ollama.
//! This module supports both HTTP-based clients and local GPU-accelerated inference.

// Engine sub-modules.
pub mod circuit_breaker;
pub mod factory;
pub mod gguf_metadata;
pub mod model_catalog_adapter;
pub mod model_storage_adapter;
pub mod models;
pub mod noop_client;
pub mod ollama_client;
pub mod sidecar_client;
pub mod sidecar_manager;
pub mod sidecar_pool;
pub mod traits;
pub mod types;

// Directory-backed sub-modules.
pub mod system;

pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
pub use factory::{
    create_llm, create_llm_with_fallback, find_local_model, is_ollama_available, LLMConfig,
};
pub use model_catalog_adapter::HardcodedModelCatalog;
pub use model_storage_adapter::FilesystemModelStorage;
pub use models::{
    get_catalog, HardwareCapabilities, ModelCatalog, ModelFamily, ModelInfo, ModelRecommender,
    PerformanceTier, Quantization, RecommendedModel,
};
pub use noop_client::NoOpLLMClient;
pub use ollama_client::OllamaClient;
pub use sidecar_client::SidecarLLMClient;
pub use sidecar_manager::{SidecarConfig, SidecarHandle, SidecarManager};
pub use sidecar_pool::{Liveness, Origin, SharedProcesses};
pub use system::{detect_capabilities, GPUInfo, GPUVendor, Platform, SystemCapabilities};
pub use traits::{GenerationConfig, LLMClient};
pub use types::*;

use crate::application::ports::model_storage::ModelStoragePort;
use crate::features::llm::dto::{DownloadModelRequestDto, DownloadModelResponseDto};
use crate::features::llm::use_cases::DownloadModelUseCase;
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
