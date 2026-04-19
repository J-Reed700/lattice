//! Model loading and initialization using mistral.rs.

use super::config::InferenceConfig;
use crate::llm::system::gpu::detect_gpu;
use crate::llm::types::LLMError;
use mistralrs::{GgufModelBuilder, Model, PagedAttentionMetaBuilder};
use std::path::Path;
use tokio::time::{timeout, Duration};

/// Model loader for GGUF models via mistral.rs.
pub struct ModelLoader {
    config: InferenceConfig,
}

impl ModelLoader {
    /// Create a new model loader.
    pub fn new(config: InferenceConfig) -> Self {
        Self { config }
    }

    /// Load a GGUF model from file.
    ///
    /// # Arguments
    /// * `model_path` - Path to the GGUF model file
    ///
    /// # Returns
    /// Loaded Model ready for inference.
    ///
    /// # Errors
    /// Returns `LLMError::ModelNotLoaded` if file doesn't exist
    /// Returns `LLMError::GenerationFailed` if loading fails
    pub async fn load<P: AsRef<Path>>(&self, model_path: P) -> Result<Model, LLMError> {
        let mut config = self.config.clone();
        if let Err(err) = config.validate() {
            return Err(LLMError::InvalidConfig(err));
        }

        if config.n_gpu_layers < 0 {
            let gpu_info = detect_gpu().await;
            let has_gpu = gpu_info
                .as_ref()
                .map(|info| info.vendor.is_accelerated())
                .unwrap_or(false);
            if !has_gpu {
                config.n_gpu_layers = 0;
            }
        }

        let model_path = model_path.as_ref();

        // Validate model file exists
        if !model_path.exists() {
            return Err(LLMError::ModelNotLoaded);
        }

        // Extract directory and filename
        let model_dir = model_path
            .parent()
            .ok_or_else(|| LLMError::InvalidConfig("Invalid model path".to_string()))?;

        let model_filename = model_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| LLMError::InvalidConfig("Invalid model filename".to_string()))?;

        // Build model using GgufModelBuilder
        let model_dir_str = model_dir.to_str().ok_or_else(|| {
            LLMError::InvalidConfig("Model path contains invalid UTF-8 characters".to_string())
        })?;

        let mut builder =
            GgufModelBuilder::new(model_dir_str, vec![model_filename.to_string()]).with_logging();

        // Log Metal GPU configuration
        tracing::info!("🚀 Initializing LLM with Metal GPU support (M3 Max optimized)",);
        tracing::info!(
            "Configuration: n_gpu_layers={}, context_size={}, batch_size={}, threads={:?}, paged_attention={}",
            config.n_gpu_layers,
            config.context_size,
            config.batch_size,
            config.n_threads,
            config.use_paged_attention
        );

        if config.n_gpu_layers == -1 {
            tracing::info!("✅ Metal GPU enabled: All layers will run on GPU (optimal for M3 Max)");
        } else if config.n_gpu_layers > 0 {
            tracing::info!(
                "⚠️  Partial GPU: {} layers on GPU, rest on CPU",
                config.n_gpu_layers
            );
        } else {
            tracing::warn!("❌ CPU-only mode: No GPU acceleration (not recommended for M3 Max)");
        }

        // Configure PagedAttention if enabled. mistralrs 97708f2 changed
        // `with_paged_attn` from a closure-returning-Result to a direct
        // `PagedAttentionConfig` value. Build the config eagerly here.
        //
        // On unsupported platforms `build()` may error; the previous
        // closure-based API would silently no-op in that case (the closure
        // wasn't called when paged attention wasn't supported). Preserve
        // that behavior: log the error, skip the `with_paged_attn` call,
        // and let model loading proceed without paged attention.
        if config.use_paged_attention {
            let block_size = config.paged_attention_block_size.unwrap_or(32);
            match PagedAttentionMetaBuilder::default()
                .with_block_size(block_size)
                .build()
            {
                Ok(paged_cfg) => {
                    builder = builder.with_paged_attn(paged_cfg);
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "PagedAttention config build failed (unsupported platform?); \
                         continuing without paged attention"
                    );
                }
            }
        }

        if config.enable_isq {
            match &config.isq_type {
                Some(isq_type) => {
                    tracing::warn!(
                        "ISQ requested ({:?}) but unsupported in current mistralrs; ignoring.",
                        isq_type
                    );
                }
                None => {
                    tracing::warn!("ISQ requested but no isq_type configured; ignoring.");
                }
            }
        }

        // Load model (fully async) with 5-minute timeout
        let model = timeout(
            Duration::from_secs(300), // 5 minutes
            builder.build(),
        )
        .await
        .map_err(|_| {
            LLMError::GenerationFailed("Model loading timed out after 5 minutes".to_string())
        })??;

        Ok(model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_loader_creation() {
        let config = InferenceConfig::default();
        let loader = ModelLoader::new(config);
        assert!(!loader.config.use_paged_attention); // Disabled by default for Metal compatibility
    }

    #[tokio::test]
    async fn test_load_nonexistent_model() {
        let config = InferenceConfig::default();
        let loader = ModelLoader::new(config);
        let result = loader.load("/nonexistent.gguf").await;
        assert!(result.is_err());
    }
}
