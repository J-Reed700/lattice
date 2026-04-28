//! Model loading and initialization using mistral.rs.

use super::config::InferenceConfig;
use super::gguf_arch;
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
        let config = self.config.clone();
        if let Err(err) = config.validate() {
            return Err(LLMError::InvalidConfig(err));
        }

        // Note: we used to call `detect_gpu()` here and overwrite
        // `n_gpu_layers` to 0 on "no GPU detected". That heuristic was
        // wrong on Apple Silicon under the Tauri sandbox (sysinfo
        // returns an empty CPU brand) and irrelevant in any case —
        // mistralrs ignores the field. Whatever value `n_gpu_layers`
        // holds is informational only.

        let model_path = model_path.as_ref();

        // Validate model file exists
        if !model_path.exists() {
            return Err(LLMError::ModelNotLoaded);
        }

        // Pre-flight: peek the GGUF header for `general.architecture` and
        // reject anything mistralrs is known to choke on. mistralrs panics
        // (`.unwrap()` in `gguf/content.rs:151`) on unknown architectures
        // and takes the whole Tauri process down with it; we cannot patch
        // the vendored crate, so we gate at the door instead.
        //
        // Run on a blocking thread: GGUFs frequently live on slow external
        // drives or NAS mounts, where even reading 1 MB can stall the
        // tokio executor for hundreds of milliseconds.
        let arch_path = model_path.to_path_buf();
        let arch = tokio::task::spawn_blocking(move || gguf_arch::read_architecture(&arch_path))
            .await
            .map_err(|e| LLMError::Other(format!("GGUF peek task panicked: {e}")))?
            .map_err(|e| {
                LLMError::Other(format!(
                    "Cannot read GGUF metadata for {}: {}",
                    model_path.display(),
                    e
                ))
            })?;

        if !gguf_arch::is_supported_architecture(&arch) {
            // Prefer a quant-source-specific hint for non-standard tags
            // (e.g. unsloth's "qwen35"), falling back to the generic
            // unsupported-arch message.
            let detail = gguf_arch::nonstandard_alias_hint(&arch).map_or_else(
                || format!(
                    "Supported architectures: {}. To use this model, configure Ollama in Settings → Chat instead.",
                    gguf_arch::SUPPORTED_ARCHITECTURES.join(", ")
                ),
                |h| h.to_string(),
            );
            return Err(LLMError::Other(format!(
                "Model architecture '{arch}' is not supported by the local LLM runtime. {detail}"
            )));
        }

        tracing::info!(arch = %arch, "GGUF architecture validated as supported");

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

        // Log inference configuration. Note: mistralrs (rev 97708f2) does
        // not have an `n_gpu_layers` API for GGUF — it auto-maps layers
        // to whatever device its `auto_device_map` selects (Metal on
        // Apple Silicon, CUDA on NVIDIA, CPU fallback otherwise). The
        // `n_gpu_layers` field on `InferenceConfig` is therefore advisory
        // only and is NOT plumbed into `GgufModelBuilder`. Look for the
        // `mistralrs_quant ... Layers N-M: <device> ...` line a few
        // lines below to see the actual device placement.
        tracing::info!(
            "Initializing LLM (mistralrs auto-device-map): context_size={}, batch_size={}, threads={:?}, paged_attention={}",
            config.context_size,
            config.batch_size,
            config.n_threads,
            config.use_paged_attention
        );

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
