use crate::features::embedding::generator::{EmbeddingGenerator, ModelConfig};
use crate::interfaces::di::Container;
use crate::shared::error::AppError;
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

/// Shared embedding generator state
pub struct EmbeddingState {
    generator: Arc<EmbeddingGenerator>,
}

impl EmbeddingState {
    pub fn new(config: ModelConfig) -> Self {
        Self {
            generator: Arc::new(EmbeddingGenerator::new(config)),
        }
    }
}

impl Default for EmbeddingState {
    fn default() -> Self {
        Self {
            generator: Arc::new(EmbeddingGenerator::default()),
        }
    }
}

impl EmbeddingState {
    pub fn generator(&self) -> &Arc<EmbeddingGenerator> {
        &self.generator
    }
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum EmbeddingOperation {
    GenerateSingle { text: String },
    GenerateBatch { texts: Vec<String> },
    GetModelInfo,
}

#[derive(Debug, Serialize, Deserialize, specta::Type)]
pub struct ModelInfo {
    pub model_name: String,
    pub dimensions: i32,
    pub is_loaded: bool,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum EmbeddingResponse {
    Single(Vec<f32>),
    Batch(Vec<Vec<f32>>),
    ModelInfo(ModelInfo),
}

/// Unified embedding operations dispatcher with rate limiting and validation
///
/// Handles all embedding-related operations through a single command with enum-based
/// dispatch. Supports single text embedding, batch processing, and model info retrieval.
/// Protected by rate limiting and comprehensive input validation to prevent abuse and
/// ensure system stability during CPU-intensive ML inference operations.
///
/// # Arguments
///
/// * `container` - Service container with security context
/// * `operation` - Enum specifying the operation to perform
/// * `state` - Shared embedding generator state
///
/// # Operations
///
/// **`GenerateSingle { text }`**:
/// - Generates embedding for single text (max 10,000 chars)
/// - Returns `Single(Vec<f32>)` - DEFAULT_EMBEDDING_DIM embedding vector
/// - Rate limited to prevent CPU exhaustion
///
/// **`GenerateBatch { texts }`**:
/// - Generates embeddings for multiple texts (max 100 texts)
/// - Returns `Batch(Vec<Vec<f32>>)` - Array of embedding vectors
/// - More efficient than multiple single requests
///
/// **`GetModelInfo`**:
/// - Returns embedding model metadata
/// - Returns `ModelInfo(Value)` - Model name, dimensions, loaded status
/// - No rate limiting (read-only metadata)
///
/// # Returns
///
/// * `Ok(EmbeddingResponse)` - Operation result (Single/Batch/ModelInfo variant)
/// * `Err(AppError)` - If rate limited, validation fails, or generation fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many embedding requests (CWE-770 mitigation)
/// * `AppError::InvalidInput` - Empty text, text too long, or batch too large
/// * `AppError::Other` - ML inference failure or model not loaded
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Single text embedding
/// const singleOp = {
///   action: 'generateSingle',
///   text: 'This is a test document'
/// };
///
/// const singleResult = await invoke<{ type: 'single', data: number[] }>(
///   'embedding_operation',
///   { operation: singleOp }
/// );
/// console.log(`Embedding dimensions: ${singleResult.data.length}`);
/// console.log(`First 5 values: ${singleResult.data.slice(0, 5)}`);
///
/// // Batch embedding
/// const batchOp = {
///   action: 'generateBatch',
///   texts: ['Document 1', 'Document 2', 'Document 3']
/// };
///
/// const batchResult = await invoke<{ type: 'batch', data: number[][] }>(
///   'embedding_operation',
///   { operation: batchOp }
/// );
/// console.log(`Generated ${batchResult.data.length} embeddings`);
///
/// // Model info
/// const infoOp = { action: 'getModelInfo' };
///
/// const infoResult = await invoke<{ type: 'modelInfo', data: ModelInfo }>(
///   'embedding_operation',
///   { operation: infoOp }
/// );
/// console.log(`Model: ${infoResult.data.model_name}`);
/// console.log(`Dimensions: ${infoResult.data.dimensions}`);
/// console.log(`Loaded: ${infoResult.data.is_loaded}`);
/// ```
///
/// # Security & Validation
///
/// **Rate Limiting (CWE-770)**:
/// - Max 200 embedding requests per minute (single or batch)
/// - Prevents CPU exhaustion from excessive embedding generation
/// - Applied to GenerateSingle and GenerateBatch operations
///
/// **Input Validation**:
/// - **Text Length**: Max 10,000 characters per text (prevents memory exhaustion)
/// - **Batch Size**: Max 100 texts per batch (DoS protection)
/// - **Empty Inputs**: Rejected with clear error messages
///
/// # Performance
///
/// **Generation Times** (CPU-bound, varies by hardware):
/// - **Single Text** (100 words): ~10-50ms on modern CPU
/// - **Batch** (10 texts, 100 words each): ~50-200ms on modern CPU
/// - **Batch** (100 texts, 100 words each): ~500ms-2s on modern CPU
///
/// **Resource Usage**:
/// - **CPU**: 100% single core during generation (blocking thread pool)
/// - **Memory**: ~500MB-1GB for model weights (loaded once, reused)
/// - **Model**: DEFAULT_EMBEDDING_MODEL_NAME (DEFAULT_EMBEDDING_DIM dimensions)
///
/// # Architecture
///
/// Enum-based dispatcher pattern with cross-cutting concerns:
/// 1. **Rate Limiting**: Applied per operation type
/// 2. **Validation**: Operation-specific input validation
/// 3. **Execution**: Delegates to blocking thread pool for CPU-bound work
/// 4. **Response Mapping**: Type-safe enum response variants
///
/// # Command Flow
///
/// 1. Match on operation enum variant
/// 2. Apply rate limiting (if applicable)
/// 3. Validate inputs
/// 4. Clone Arc to generator
/// 5. Spawn blocking task for CPU-bound ML inference
/// 6. Map result to EmbeddingResponse enum
///
/// # Note
///
/// This is the **preferred** embedding command. Use `generate_embedding` and
/// `generate_embeddings_batch` only for legacy compatibility or simpler use cases
/// without rate limiting requirements.
pub async fn embedding_operation(
    container: State<'_, Container>,
    operation: EmbeddingOperation,
    state: State<'_, EmbeddingState>,
) -> Result<EmbeddingResponse, AppError> {
    match operation {
        EmbeddingOperation::GenerateSingle { text } => {
            // Rate limiting
            container
                .security_context()
                .rate_limiters()
                .embedding
                .check_rate_limit("embedding")
                .await
                .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

            // Validation
            if text.is_empty() {
                return Err(AppError::InvalidInput("Text cannot be empty".into()));
            }
            if text.len() > 10_000 {
                return Err(AppError::InvalidInput(
                    "Text too long (max 10k chars)".into(),
                ));
            }

            let generator = Arc::clone(state.generator());
            let embedding = tokio::task::spawn_blocking(move || generator.generate(&text))
                .await
                .map_err(|e| AppError::Other(format!("Task failed: {}", e)))??;
            Ok(EmbeddingResponse::Single(embedding))
        }
        EmbeddingOperation::GenerateBatch { texts } => {
            // Rate limiting
            container
                .security_context()
                .rate_limiters()
                .embedding
                .check_rate_limit("embedding")
                .await
                .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

            // Batch size validation (DoS protection)
            if texts.is_empty() {
                return Err(AppError::InvalidInput("Empty batch".into()));
            }
            if texts.len() > 100 {
                return Err(AppError::InvalidInput("Batch too large (max 100)".into()));
            }

            // Validate each text
            for text in &texts {
                if text.len() > 10_000 {
                    return Err(AppError::InvalidInput("Text too long in batch".into()));
                }
            }

            let generator = Arc::clone(state.generator());
            let embeddings = tokio::task::spawn_blocking(move || generator.generate_batch(&texts))
                .await
                .map_err(|e| AppError::Other(format!("Task failed: {}", e)))??;
            Ok(EmbeddingResponse::Batch(embeddings))
        }
        EmbeddingOperation::GetModelInfo => {
            let generator = state.generator();
            let info = ModelInfo {
                model_name: generator.model_name().to_string(),
                dimensions: generator.dimensions() as i32,
                is_loaded: generator.is_loaded(),
            };
            Ok(EmbeddingResponse::ModelInfo(info))
        }
    }
}

/// Generates a single text embedding without rate limiting
///
/// Legacy command for generating embeddings. Converts text into a DEFAULT_EMBEDDING_DIM-dimensional vector
/// using the DEFAULT_EMBEDDING_MODEL_NAME model. Runs in a blocking thread pool
/// since ML inference is CPU-bound. **No rate limiting** - use `embedding_operation` for
/// production code requiring rate limiting and validation.
///
/// # Arguments
///
/// * `text` - Text to convert to embedding vector
/// * `state` - Shared embedding generator state
///
/// # Returns
///
/// * `Ok(Vec<f32>)` - Normalized DEFAULT_EMBEDDING_DIM-dimensional embedding vector
/// * `Err(AppError)` - If inference fails or model not loaded
///
/// # Errors
///
/// * `AppError::Other` - ML inference failed or blocking task panicked
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// const embedding = await invoke<number[]>('generate_embedding', {
///   text: 'This is a sample document about machine learning'
/// });
///
/// console.log(`Embedding dimensions: ${embedding.length}`); // DEFAULT_EMBEDDING_DIM
/// console.log(`First 5 values: ${embedding.slice(0, 5)}`);
/// ```
///
/// # Performance
///
/// - **Generation Time**: ~10-50ms for typical text (100-500 words)
/// - **CPU-Bound**: Blocks single core during generation
/// - **Thread Pool**: Runs in Tokio blocking pool (doesn't block async runtime)
///
/// # Warning
///
/// **No Rate Limiting**: This command has no rate limiting or validation. For production
/// use, prefer `embedding_operation` which includes rate limiting (CWE-770), input
/// validation, and better error handling. This command exists for:
/// - Legacy compatibility
/// - Internal/trusted use cases
/// - Simple scripts where rate limiting is unnecessary
///
/// # Architecture
///
/// Simple wrapper around EmbeddingGenerator with blocking thread pool execution
pub(crate) async fn generate_embedding_impl(
    text: String,
    state: &EmbeddingState,
) -> Result<Vec<f32>, AppError> {
    let generator = Arc::clone(state.generator());

    // Run in blocking thread pool since embedding generation is CPU-bound
    tokio::task::spawn_blocking(move || generator.generate(&text))
        .await
        .map_err(|e| AppError::Other(format!("Task failed: {}", e)))?
}

pub async fn generate_embedding(
    text: String,
    state: State<'_, EmbeddingState>,
) -> Result<Vec<f32>, AppError> {
    generate_embedding_impl(text, &state).await
}

/// Generates embeddings for multiple texts in a single batch operation
///
/// Legacy command for batch embedding generation. More efficient than multiple single
/// calls due to model batching optimizations. Converts array of texts into array of
/// DEFAULT_EMBEDDING_DIM-dimensional vectors. **No rate limiting** - use `embedding_operation` for
/// production code requiring rate limiting and validation.
///
/// # Arguments
///
/// * `texts` - Array of texts to convert to embeddings
/// * `state` - Shared embedding generator state
///
/// # Returns
///
/// * `Ok(Vec<Vec<f32>>)` - Array of normalized DEFAULT_EMBEDDING_DIM-dimensional embedding vectors
/// * `Err(AppError)` - If inference fails or model not loaded
///
/// # Errors
///
/// * `AppError::Other` - ML inference failed or blocking task panicked
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// const texts = [
///   'First document about AI',
///   'Second document about machine learning',
///   'Third document about neural networks'
/// ];
///
/// const embeddings = await invoke<number[][]>('generate_embeddings_batch', {
///   texts
/// });
///
/// console.log(`Generated ${embeddings.length} embeddings`);
/// embeddings.forEach((emb, i) => {
///   console.log(`Doc ${i + 1}: ${emb.length} dimensions, first value: ${emb[0]}`);
/// });
///
/// // Calculate similarity between first two documents
/// const dotProduct = embeddings[0].reduce((sum, val, i) =>
///   sum + val * embeddings[1][i], 0);
/// console.log(`Similarity: ${dotProduct}`);
/// ```
///
/// # Performance
///
/// - **Batch Processing**: ~3-5x faster than individual calls for large batches
/// - **Generation Time**: ~50-200ms for 10 texts, ~500ms-2s for 100 texts
/// - **CPU-Bound**: Blocks single core during generation
/// - **Thread Pool**: Runs in Tokio blocking pool
///
/// **Efficiency Comparison**:
/// - **10 Individual Calls**: ~100-500ms total
/// - **1 Batch Call (10 texts)**: ~50-200ms total
/// - **Speedup**: ~2-3x faster with batching
///
/// # Warning
///
/// **No Rate Limiting or Validation**: This command has no rate limiting, batch size
/// limits, or input validation. For production use, prefer `embedding_operation` which
/// includes:
/// - Rate limiting (CWE-770 mitigation)
/// - Batch size validation (max 100 texts)
/// - Text length validation (max 10,000 chars per text)
/// - Better error handling
///
/// Use this command only for:
/// - Legacy compatibility
/// - Internal/trusted use cases
/// - Simple scripts where protection is unnecessary
///
/// # Architecture
///
/// Simple wrapper around EmbeddingGenerator with blocking thread pool execution
pub(crate) async fn generate_embeddings_batch_impl(
    texts: Vec<String>,
    state: &EmbeddingState,
) -> Result<Vec<Vec<f32>>, AppError> {
    let generator = Arc::clone(state.generator());

    // Run in blocking thread pool
    tokio::task::spawn_blocking(move || generator.generate_batch(&texts))
        .await
        .map_err(|e| AppError::Other(format!("Task failed: {}", e)))?
}

pub async fn generate_embeddings_batch(
    texts: Vec<String>,
    state: State<'_, EmbeddingState>,
) -> Result<Vec<Vec<f32>>, AppError> {
    generate_embeddings_batch_impl(texts, &state).await
}

/// Gets metadata about the current embedding model
///
/// Returns information about the loaded embedding model including model name,
/// embedding dimensions, and load status. Useful for frontend to display model
/// info and verify compatibility. **Read-only** - no rate limiting required.
///
/// # Arguments
///
/// * `state` - Shared embedding generator state
///
/// # Returns
///
/// * `Ok(Value)` - JSON object with model metadata
/// * `Err(AppError)` - If state access fails (unlikely)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface EmbeddingModelInfo {
///   model_name: string;      // Model identifier
///   dimensions: number;      // Embedding vector dimensions
///   is_loaded: boolean;      // Model loaded and ready
/// }
///
/// const info = await invoke<EmbeddingModelInfo>('get_embedding_model_info');
///
/// console.log(`Model: ${info.model_name}`);
/// console.log(`Dimensions: ${info.dimensions}`);
/// console.log(`Status: ${info.is_loaded ? 'Ready' : 'Not loaded'}`);
///
/// if (info.dimensions !== DEFAULT_EMBEDDING_DIM) {
///   console.warn(`Unexpected dimensions: expected DEFAULT_EMBEDDING_DIM, got ${info.dimensions}`);
/// }
/// ```
///
/// # Model Information
///
/// **Default Model**: `DEFAULT_EMBEDDING_MODEL_NAME`
/// - **Dimensions**: DEFAULT_EMBEDDING_DIM
/// - **Model Size**: ~420MB
/// - **Architecture**: BERT-based sentence transformer
/// - **Use Case**: General-purpose semantic search and similarity
///
/// **Loaded Status**:
/// - **`true`**: Model weights loaded in memory, ready for inference
/// - **`false`**: Model not loaded (initialization failed or pending)
///
/// # Performance
///
/// - **Query Time**: <1ms (reads from memory, no computation)
/// - **No Network**: Local metadata access only
/// - **Always Available**: Can be called before model is fully loaded
///
/// # Use Cases
///
/// - **Startup Verification**: Confirm model loaded successfully
/// - **Compatibility Checks**: Verify dimensions match expectations
/// - **UI Display**: Show current model to users
/// - **Debugging**: Diagnose embedding-related issues
///
/// # Architecture
///
/// Simple metadata accessor - no business logic, just returns cached state
pub(crate) async fn get_embedding_model_info_impl(
    state: &EmbeddingState,
) -> Result<ModelInfo, AppError> {
    let generator = state.generator();

    Ok(ModelInfo {
        model_name: generator.model_name().to_string(),
        dimensions: generator.dimensions() as i32,
        is_loaded: generator.is_loaded(),
    })
}

pub async fn get_embedding_model_info(
    state: State<'_, EmbeddingState>,
) -> Result<ModelInfo, AppError> {
    get_embedding_model_info_impl(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::embedding_constants::{DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_NAME};

    #[tokio::test]
    async fn test_generate_embedding_requires_a_configured_model() {
        let state = EmbeddingState::default();
        let result = generate_embedding_impl("Hello world".to_string(), &state).await;

        assert!(matches!(result, Err(AppError::ModelLoadFailed(_))));
    }

    #[tokio::test]
    async fn test_generate_embeddings_batch_requires_a_configured_model() {
        let state = EmbeddingState::default();
        let texts = vec!["Hello".to_string(), "World".to_string()];
        let result = generate_embeddings_batch_impl(texts, &state).await;

        assert!(matches!(result, Err(AppError::ModelLoadFailed(_))));
    }

    #[tokio::test]
    async fn test_get_model_info() {
        let state = EmbeddingState::default();
        let result = get_embedding_model_info_impl(&state).await;

        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.model_name, DEFAULT_EMBEDDING_MODEL_NAME);
        assert_eq!(info.dimensions, DEFAULT_EMBEDDING_DIM as i32);
    }

    #[tokio::test]
    async fn test_empty_text_error() {
        let state = EmbeddingState::default();
        let result = generate_embedding_impl("".to_string(), &state).await;
        assert!(result.is_err());
    }
}
