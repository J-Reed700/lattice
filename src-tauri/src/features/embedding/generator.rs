//! Legacy thin embedding generator API.
//!
//! Historically wrapped `fastembed::TextEmbedding` for the standalone
//! `generate_embedding` Tauri command. Now a Candle-backed shim that
//! lazy-loads a model on first use.
//!
//! This API is preserved for backward compatibility with the existing
//! Tauri command surface. New callers should go through
//! `CandleEmbeddingService` directly via the active embedding service in
//! the DI container.

use std::path::PathBuf;
use std::sync::Arc;

use once_cell::sync::OnceCell;
use parking_lot::Mutex;

use crate::domain::embedding_constants::{DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_NAME};
use crate::features::embedding::candle_service::CandleEmbeddingService;
use crate::shared::error::{AppError, Result};

/// Embedding model configuration. Kept as an enum for API compatibility
/// with the previous fastembed-backed generator; today we don't pre-bind to
/// any specific architecture — the model is whatever the caller's
/// `model_dir` contains.
#[derive(Debug, Clone, Default)]
pub enum ModelConfig {
    /// Load whatever model is at this directory (config.json, tokenizer.json,
    /// model.safetensors).
    Local(PathBuf),
    /// Default placeholder used when no explicit config is provided.
    /// Returns an error on first generate() call until configured.
    #[default]
    Unset,
}

impl ModelConfig {
    pub fn dimensions(&self) -> usize {
        DEFAULT_EMBEDDING_DIM
    }

    pub fn model_name(&self) -> &'static str {
        DEFAULT_EMBEDDING_MODEL_NAME
    }
}

/// Lazy single-model embedding generator.
///
/// This intentionally serializes inference behind a `Mutex` — the underlying
/// Candle model isn't designed for concurrent forward passes from multiple
/// threads. For high-throughput use the indexing pipeline batches via the
/// container's embedding service.
pub struct EmbeddingGenerator {
    model: OnceCell<Mutex<Option<Arc<CandleEmbeddingService>>>>,
    config: ModelConfig,
}

impl EmbeddingGenerator {
    /// Create a new generator with lazy loading.
    pub fn new(config: ModelConfig) -> Self {
        Self {
            model: OnceCell::new(),
            config,
        }
    }

    /// True once the underlying Candle model has been loaded.
    pub fn is_loaded(&self) -> bool {
        self.model
            .get()
            .map(|cell| cell.lock().is_some())
            .unwrap_or(false)
    }

    pub fn dimensions(&self) -> usize {
        self.model
            .get()
            .and_then(|cell| cell.lock().as_ref().map(|svc| svc.dimension()))
            .unwrap_or_else(|| self.config.dimensions())
    }

    pub fn model_name(&self) -> &'static str {
        self.config.model_name()
    }

    fn load_if_needed(&self) -> Result<Arc<CandleEmbeddingService>> {
        let cell = self.model.get_or_init(|| Mutex::new(None));
        let mut guard = cell.lock();
        if let Some(existing) = guard.as_ref() {
            return Ok(existing.clone());
        }

        let dir = match &self.config {
            ModelConfig::Local(dir) => dir.clone(),
            ModelConfig::Unset => {
                return Err(AppError::ModelLoadFailed(
                    "EmbeddingGenerator was created without a model path. \
                     Use CandleEmbeddingService directly or supply ModelConfig::Local."
                        .to_string(),
                ));
            }
        };

        let svc = Arc::new(CandleEmbeddingService::open_unregistered(&dir)?);
        *guard = Some(svc.clone());
        Ok(svc)
    }

    pub fn generate(&self, text: &str) -> Result<Vec<f32>> {
        let svc = self.load_if_needed()?;
        // EmbeddingPort::embed_single is async; bridge by blocking the current
        // thread on a tokio runtime handle. Callers are already inside
        // spawn_blocking (Tauri command pattern) so this is fine.
        let rt = tokio::runtime::Handle::try_current().map_err(|_| AppError::EmbeddingFailed {
            reason: "EmbeddingGenerator::generate called outside a tokio runtime".into(),
        })?;
        let text = text.to_string();
        rt.block_on(async move {
            <CandleEmbeddingService as crate::application::ports::EmbeddingPort>::embed_single(
                &svc, &text,
            )
            .await
        })
    }

    pub fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let svc = self.load_if_needed()?;
        let rt = tokio::runtime::Handle::try_current().map_err(|_| AppError::EmbeddingFailed {
            reason: "EmbeddingGenerator::generate_batch called outside a tokio runtime".into(),
        })?;
        let texts = texts.to_vec();
        rt.block_on(async move {
            <CandleEmbeddingService as crate::application::ports::EmbeddingPort>::embed_batch(
                &svc, &texts,
            )
            .await
        })
    }
}

impl Default for EmbeddingGenerator {
    fn default() -> Self {
        Self::new(ModelConfig::default())
    }
}
