//! fastembed-backed embedding service.
//!
//! This keeps the existing `OnnxEmbeddingService` API surface but delegates
//! embedding creation to `fastembed` so we avoid hand-rolled ONNX plumbing.

use crate::application::ports::EmbeddingPort;
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
use crate::infrastructure::services::traits::EmbeddingServiceTrait;
use crate::shared::constants::ONNX_INFERENCE_TIMEOUT;
use crate::shared::error::AppError;
use crate::shared::result::Result;
use async_trait::async_trait;
use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};
use once_cell::sync::OnceCell;
use ort::execution_providers::{
    CPUExecutionProvider, CUDAExecutionProvider, CoreMLExecutionProvider, ExecutionProviderDispatch,
};
use parking_lot::Mutex;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const EMBEDDING_DIMENSION: usize = DEFAULT_EMBEDDING_DIM;
const MAX_SEQUENCE_LENGTH: usize = 512;
// Large document indexing can produce hundreds of chunks. Splitting inference
// requests keeps each ONNX call below the per-call timeout threshold.
const MAX_TEXTS_PER_INFERENCE_BATCH: usize = 16;

static FASTEMBED_MODEL_CACHE: OnceCell<Mutex<HashMap<PathBuf, Arc<Mutex<TextEmbedding>>>>> =
    OnceCell::new();

fn read_required_file(path: &Path, name: &str) -> Result<Vec<u8>> {
    if !path.exists() {
        return Err(AppError::NotFound(format!(
            "Required model file not found: {} ({})",
            path.display(),
            name
        )));
    }

    std::fs::read(path).map_err(|error| AppError::FileRead {
        path: path.display().to_string(),
        reason: error.to_string(),
    })
}

fn read_optional_json_with_default(
    path: &Path,
    name: &str,
    default_value: serde_json::Value,
) -> Result<Vec<u8>> {
    if path.exists() {
        return read_required_file(path, name);
    }

    tracing::warn!(
        "Missing {} at {}, using synthesized defaults for compatibility",
        name,
        path.display()
    );

    serde_json::to_vec(&default_value).map_err(|error| {
        AppError::Serialization(format!(
            "Failed to serialize synthesized {} defaults: {}",
            name, error
        ))
    })
}

fn infer_tokenizer_defaults(tokenizer_file: &[u8]) -> (u32, String, usize) {
    let mut pad_id = 0u32;
    let mut pad_token = "[PAD]".to_string();
    let mut max_length = MAX_SEQUENCE_LENGTH;

    if let Ok(tokenizer_json) = serde_json::from_slice::<serde_json::Value>(tokenizer_file) {
        if let Some(padding) = tokenizer_json.get("padding") {
            if let Some(parsed_pad_id) = padding.get("pad_id").and_then(|value| value.as_u64()) {
                pad_id = parsed_pad_id as u32;
            }
            if let Some(parsed_pad_token) =
                padding.get("pad_token").and_then(|value| value.as_str())
            {
                pad_token = parsed_pad_token.to_string();
            }
        }

        if let Some(truncation) = tokenizer_json.get("truncation") {
            if let Some(parsed_max_length) = truncation
                .get("max_length")
                .and_then(|value| value.as_u64())
            {
                max_length = parsed_max_length as usize;
            }
        }
    }

    (pad_id, pad_token, max_length.max(1))
}

fn execution_provider_candidates() -> Vec<Vec<ExecutionProviderDispatch>> {
    let mut candidates = Vec::new();

    #[cfg(target_os = "macos")]
    {
        candidates.push(vec![
            CoreMLExecutionProvider::default().build(),
            CPUExecutionProvider::default().build(),
        ]);
    }

    candidates.push(vec![
        CUDAExecutionProvider::default().build(),
        CPUExecutionProvider::default().build(),
    ]);

    candidates.push(vec![CPUExecutionProvider::default().build()]);
    candidates
}

fn load_fastembed_model(model_path: &Path) -> Result<TextEmbedding> {
    if !model_path.exists() {
        return Err(AppError::NotFound(format!(
            "ONNX model file not found: {}",
            model_path.display()
        )));
    }

    let model_dir = model_path
        .parent()
        .ok_or_else(|| AppError::EmbeddingFailed {
            reason: "Failed to resolve model directory".to_string(),
        })?;

    let onnx_file = read_required_file(model_path, "model.onnx")?;
    let tokenizer_file = read_required_file(&model_dir.join("tokenizer.json"), "tokenizer.json")?;
    let (pad_id, pad_token, inferred_max_length) = infer_tokenizer_defaults(&tokenizer_file);
    let tokenizer_files = TokenizerFiles {
        tokenizer_file,
        config_file: read_optional_json_with_default(
            &model_dir.join("config.json"),
            "config.json",
            json!({ "pad_token_id": pad_id }),
        )?,
        special_tokens_map_file: read_optional_json_with_default(
            &model_dir.join("special_tokens_map.json"),
            "special_tokens_map.json",
            json!({}),
        )?,
        tokenizer_config_file: read_optional_json_with_default(
            &model_dir.join("tokenizer_config.json"),
            "tokenizer_config.json",
            json!({
                "model_max_length": inferred_max_length,
                "pad_token": pad_token
            }),
        )?,
    };

    let user_defined_model =
        UserDefinedEmbeddingModel::new(onnx_file, tokenizer_files).with_pooling(Pooling::Mean);

    let mut last_error = None;
    for providers in execution_provider_candidates() {
        let options = InitOptionsUserDefined::new()
            .with_execution_providers(providers)
            .with_max_length(MAX_SEQUENCE_LENGTH);

        match TextEmbedding::try_new_from_user_defined(user_defined_model.clone(), options) {
            Ok(model) => {
                tracing::info!(
                    "Initialized fastembed model from local ONNX: {}",
                    model_path.display()
                );
                return Ok(model);
            }
            Err(error) => {
                last_error = Some(error.to_string());
            }
        }
    }

    Err(AppError::EmbeddingFailed {
        reason: format!(
            "Failed to initialize embedding model from {}: {}",
            model_path.display(),
            last_error.unwrap_or_else(|| "unknown fastembed initialization error".to_string())
        ),
    })
}

fn get_or_create_fastembed_model(
    model_path: impl AsRef<Path>,
) -> Result<Arc<Mutex<TextEmbedding>>> {
    let model_path_ref = model_path.as_ref();
    let cache_key =
        std::fs::canonicalize(model_path_ref).unwrap_or_else(|_| model_path_ref.to_path_buf());

    let cache = FASTEMBED_MODEL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some(existing) = cache.lock().get(&cache_key) {
        tracing::debug!("Using cached fastembed model: {}", cache_key.display());
        return Ok(Arc::clone(existing));
    }

    let model = Arc::new(Mutex::new(load_fastembed_model(&cache_key)?));

    let mut guard = cache.lock();
    let entry = guard
        .entry(cache_key.clone())
        .or_insert_with(|| Arc::clone(&model));

    tracing::info!("Cached fastembed model: {}", cache_key.display());
    Ok(Arc::clone(entry))
}

/// Local embedding service backed by fastembed.
pub struct OnnxEmbeddingService {
    model: Arc<Mutex<TextEmbedding>>,
    dimension: usize,
}

impl OnnxEmbeddingService {
    /// Create a new embedding service from a local ONNX model path.
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self> {
        let model = get_or_create_fastembed_model(model_path)?;

        Ok(Self {
            model,
            dimension: EMBEDDING_DIMENSION,
        })
    }

    /// Generate embeddings for contextualized chunks.
    pub async fn embed_contextualized_chunks(
        &self,
        chunks: &[crate::infrastructure::indexing::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        if chunks.is_empty() {
            return Ok(vec![]);
        }

        let texts: Vec<String> = chunks
            .iter()
            .map(|chunk| chunk.contextualized_content.clone())
            .collect();

        <Self as EmbeddingPort>::embed_batch(self, &texts).await
    }

    async fn embed_text_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let batch_size = texts.len();
        let model = Arc::clone(&self.model);

        let embeddings = tokio::time::timeout(
            ONNX_INFERENCE_TIMEOUT,
            tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>> {
                let mut guard = model.lock();
                guard
                    .embed(texts, None)
                    .map_err(|error| AppError::EmbeddingFailed {
                        reason: format!("Batch embedding generation failed: {}", error),
                    })
            }),
        )
        .await
        .map_err(|_| AppError::EmbeddingFailed {
            reason: format!(
                "Embedding inference timed out after {} seconds (batch size: {})",
                ONNX_INFERENCE_TIMEOUT.as_secs(),
                batch_size
            ),
        })?
        .map_err(|error| AppError::EmbeddingFailed {
            reason: format!("Embedding task panicked: {}", error),
        })??;

        Ok(embeddings)
    }
}

#[async_trait]
impl EmbeddingPort for OnnxEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        if text.is_empty() {
            return Err(AppError::InvalidInput(
                "Cannot embed empty text".to_string(),
            ));
        }

        let text_owned = vec![text.to_string()];
        let model = Arc::clone(&self.model);

        let mut embeddings = tokio::time::timeout(
            ONNX_INFERENCE_TIMEOUT,
            tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>> {
                let mut guard = model.lock();
                guard
                    .embed(text_owned, None)
                    .map_err(|error| AppError::EmbeddingFailed {
                        reason: format!("Embedding generation failed: {}", error),
                    })
            }),
        )
        .await
        .map_err(|_| AppError::EmbeddingFailed {
            reason: format!(
                "Embedding inference timed out after {} seconds",
                ONNX_INFERENCE_TIMEOUT.as_secs()
            ),
        })?
        .map_err(|error| AppError::EmbeddingFailed {
            reason: format!("Embedding task panicked: {}", error),
        })??;

        let embedding = embeddings.pop().ok_or_else(|| AppError::EmbeddingFailed {
            reason: "No embedding returned from inference".to_string(),
        })?;

        if embedding.len() != self.dimension {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    self.dimension,
                    embedding.len()
                ),
            });
        }

        Ok(embedding)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        if let Some((index, _)) = texts.iter().enumerate().find(|(_, text)| text.is_empty()) {
            return Err(AppError::InvalidInput(format!(
                "Cannot embed empty text at index {}",
                index
            )));
        }

        let mut embeddings = Vec::with_capacity(texts.len());

        for text_batch in texts.chunks(MAX_TEXTS_PER_INFERENCE_BATCH) {
            let batch_offset = embeddings.len();
            let batch_embeddings = self.embed_text_batch(text_batch.to_vec()).await?;

            if let Some((index, embedding)) = batch_embeddings
                .iter()
                .enumerate()
                .find(|(_, embedding)| embedding.len() != self.dimension)
            {
                return Err(AppError::EmbeddingFailed {
                    reason: format!(
                        "Embedding dimension mismatch at index {}: expected {}, got {}",
                        batch_offset + index,
                        self.dimension,
                        embedding.len()
                    ),
                });
            }

            embeddings.extend(batch_embeddings);
        }

        if embeddings.len() != texts.len() {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "Embedding count mismatch: expected {}, got {}",
                    texts.len(),
                    embeddings.len()
                ),
            });
        }

        Ok(embeddings)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[async_trait]
impl EmbeddingServiceTrait for OnnxEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        <Self as EmbeddingPort>::embed_single(self, text).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        <Self as EmbeddingPort>::embed_batch(self, texts).await
    }

    async fn embed_contextualized_chunks(
        &self,
        chunks: &[crate::infrastructure::indexing::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        Self::embed_contextualized_chunks(self, chunks).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_model_not_found() {
        let result = OnnxEmbeddingService::new("nonexistent.onnx");
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    #[test]
    fn test_new_supports_missing_tokenizer_companion_files(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let model_path = temp_dir.path().join("model.onnx");
        let tokenizer_path = temp_dir.path().join("tokenizer.json");
        std::fs::write(&model_path, b"not-a-real-model")?;
        std::fs::write(&tokenizer_path, b"{}")?;

        let result = OnnxEmbeddingService::new(&model_path);
        // Missing companion files should no longer hard-fail with NotFound.
        assert!(!matches!(result, Err(AppError::NotFound(_))));

        Ok(())
    }

    #[test]
    fn test_service_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<OnnxEmbeddingService>();
    }
}
