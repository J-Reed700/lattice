use crate::domain::embedding_constants::{DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_NAME};
use crate::shared::error::{AppError, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use once_cell::sync::OnceCell;
use std::sync::Mutex;

/// Embedding model configuration
#[derive(Debug, Clone, Copy)]
pub enum ModelConfig {
    /// DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME: DEFAULT_EMBEDDING_DIM dimensions
    AllMpnetBaseV2,
}

impl ModelConfig {
    pub fn dimensions(&self) -> usize {
        match self {
            Self::AllMpnetBaseV2 => DEFAULT_EMBEDDING_DIM,
        }
    }

    pub fn model_name(&self) -> &'static str {
        match self {
            Self::AllMpnetBaseV2 => DEFAULT_EMBEDDING_MODEL_NAME,
        }
    }

    fn to_fastembed_model(self) -> EmbeddingModel {
        match self {
            Self::AllMpnetBaseV2 => EmbeddingModel::AllMpnetBaseV2,
        }
    }
}

/// High-performance embedding generator using fastembed-rs
///
/// This implementation uses ONNX Runtime for improved inference performance
/// compared to Python's sentence-transformers.
pub struct EmbeddingGenerator {
    model: OnceCell<Mutex<TextEmbedding>>,
    config: ModelConfig,
}

impl EmbeddingGenerator {
    /// Create a new generator with lazy loading
    pub fn new(config: ModelConfig) -> Self {
        Self {
            model: OnceCell::new(),
            config,
        }
    }
}

impl Default for EmbeddingGenerator {
    /// Create with default configuration (DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME)
    fn default() -> Self {
        Self::new(ModelConfig::AllMpnetBaseV2)
    }
}

impl EmbeddingGenerator {
    /// Get or initialize the model
    fn get_model(&self) -> Result<&Mutex<TextEmbedding>> {
        self.model.get_or_try_init(|| {
            tracing::info!(
                "Loading embedding model: {} ({} dimensions)",
                self.config.model_name(),
                self.config.dimensions()
            );

            let model = TextEmbedding::try_new(
                InitOptions::new(self.config.to_fastembed_model())
                    .with_show_download_progress(false),
            )
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("Failed to load model {}: {}", self.config.model_name(), e),
            })?;

            tracing::info!("Embedding model loaded successfully");
            Ok(Mutex::new(model))
        })
    }

    /// Check if model is loaded
    pub fn is_loaded(&self) -> bool {
        self.model.get().is_some()
    }

    /// Get the embedding dimensions for this model
    pub fn dimensions(&self) -> usize {
        self.config.dimensions()
    }

    /// Get the model name
    pub fn model_name(&self) -> &'static str {
        self.config.model_name()
    }

    /// Generate embedding for a single text
    ///
    /// # Arguments
    /// * `text` - Text to embed (must not be empty)
    ///
    /// # Returns
    /// * `Ok(Vec<f32>)` - Normalized embedding vector
    /// * `Err(AppError)` - If text is empty or generation fails
    pub fn generate(&self, text: &str) -> Result<Vec<f32>> {
        if text.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "text: Text cannot be empty".to_string(),
            ));
        }

        let model = self.get_model()?;

        let embeddings = model
            .lock()
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("Failed to acquire model lock: {}", e),
            })?
            .embed(vec![text], None)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("Embedding generation failed: {}", e),
            })?;

        let mut embedding =
            embeddings
                .into_iter()
                .next()
                .ok_or_else(|| AppError::EmbeddingFailed {
                    reason: "No embedding generated".to_string(),
                })?;

        // Normalize the embedding
        normalize_embedding(&mut embedding);

        // Validate dimension
        if embedding.len() != self.dimensions() {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    self.dimensions(),
                    embedding.len()
                ),
            });
        }

        Ok(embedding)
    }

    /// Generate embeddings for multiple texts in batch
    ///
    /// This is more efficient than calling `generate` multiple times
    /// as it processes texts in parallel.
    ///
    /// # Arguments
    /// * `texts` - Slice of texts to embed (must not be empty)
    ///
    /// # Returns
    /// * `Ok(Vec<Vec<f32>>)` - Normalized embedding vectors
    /// * `Err(AppError)` - If any text is empty or generation fails
    pub fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // Validate all texts
        for (i, text) in texts.iter().enumerate() {
            if text.trim().is_empty() {
                return Err(AppError::InvalidInput(format!(
                    "texts[{}]: Text cannot be empty",
                    i
                )));
            }
        }

        let model = self.get_model()?;

        let texts_ref: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        let mut embeddings = model
            .lock()
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("Failed to acquire model lock: {}", e),
            })?
            .embed(texts_ref, None)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("Batch embedding generation failed: {}", e),
            })?;

        // Normalize all embeddings
        for embedding in &mut embeddings {
            normalize_embedding(embedding);

            // Validate dimension
            if embedding.len() != self.dimensions() {
                return Err(AppError::EmbeddingFailed {
                    reason: format!(
                        "Embedding dimension mismatch: expected {}, got {}",
                        self.dimensions(),
                        embedding.len()
                    ),
                });
            }
        }

        Ok(embeddings)
    }
}

/// Normalize an embedding vector in-place using L2 normalization
fn normalize_embedding(embedding: &mut [f32]) {
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in embedding.iter_mut() {
            *val /= norm;
        }
    }
}
