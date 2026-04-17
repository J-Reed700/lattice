//! Inference configuration for local LLM execution using mistral.rs.

use serde::{Deserialize, Serialize};

/// ISQ quantization types supported by mistral.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IsqType {
    Q4K,
    Q8_0,
}

/// Configuration for inference engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceConfig {
    /// Number of layers to offload to GPU (-1 = all, 0 = CPU only)
    pub n_gpu_layers: i32,

    /// Context window size in tokens
    #[serde(default = "default_context_size")]
    pub context_size: u32,

    /// Batch size for prompt processing
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,

    /// Number of CPU threads (None = auto-detect)
    pub n_threads: Option<u32>,

    /// Enable PagedAttention for memory efficiency
    #[serde(default = "default_use_paged_attention")]
    pub use_paged_attention: bool,

    /// Block size for PagedAttention (in tokens)
    #[serde(default = "default_paged_attention_block_size")]
    pub paged_attention_block_size: Option<usize>,

    /// Enable ISQ (In-Situ Quantization) for faster loading on Metal
    #[serde(default)]
    pub enable_isq: bool,

    /// ISQ quantization type (Q4K, Q8_0, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isq_type: Option<IsqType>,

    /// Rope frequency base (for extended context)
    pub rope_freq_base: Option<f32>,

    /// Rope frequency scale (for extended context)
    pub rope_freq_scale: Option<f32>,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            n_gpu_layers: 0, // CPU-only to bypass Metal backend bugs (v0.6.0 compatibility issue)
            context_size: default_context_size(),
            batch_size: default_batch_size(),
            n_threads: None,
            use_paged_attention: default_use_paged_attention(),
            paged_attention_block_size: default_paged_attention_block_size(),
            enable_isq: false,
            isq_type: None,
            rope_freq_base: None,
            rope_freq_scale: None,
        }
    }
}

impl InferenceConfig {
    /// Create a CPU-only configuration.
    pub fn cpu_only() -> Self {
        Self {
            n_gpu_layers: 0,
            ..Default::default()
        }
    }

    /// Create a GPU-accelerated configuration.
    ///
    /// # Arguments
    /// * `n_gpu_layers` - Number of layers to offload (use -1 for all)
    pub fn with_gpu(n_gpu_layers: i32) -> Self {
        Self {
            n_gpu_layers,
            ..Default::default()
        }
    }

    /// Set context size.
    pub fn with_context_size(mut self, context_size: u32) -> Self {
        self.context_size = context_size;
        self
    }

    /// Set number of threads.
    pub fn with_threads(mut self, n_threads: u32) -> Self {
        self.n_threads = Some(n_threads);
        self
    }

    /// Set batch size.
    pub fn with_batch_size(mut self, batch_size: u32) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Enable ISQ quantization for faster Metal loading.
    pub fn with_isq(mut self, isq_type: IsqType) -> Self {
        self.enable_isq = true;
        self.isq_type = Some(isq_type);
        self
    }

    /// Validate configuration parameters.
    pub fn validate(&self) -> Result<(), String> {
        if self.context_size == 0 {
            return Err("context_size must be greater than 0".to_string());
        }
        if self.batch_size == 0 {
            return Err("batch_size must be greater than 0".to_string());
        }
        if self.context_size < self.batch_size {
            return Err("context_size should be >= batch_size".to_string());
        }
        Ok(())
    }
}

fn default_context_size() -> u32 {
    4096
}

fn default_batch_size() -> u32 {
    512
}

fn default_use_paged_attention() -> bool {
    false // Disabled by default for Metal GPU compatibility (conservative approach)
}

fn default_paged_attention_block_size() -> Option<usize> {
    Some(32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = InferenceConfig::default();
        assert_eq!(config.n_gpu_layers, 0); // CPU-only by default due to Metal backend bugs
        assert_eq!(config.context_size, 4096);
        assert!(!config.use_paged_attention); // Disabled by default for Metal compatibility
        assert_eq!(config.paged_attention_block_size, Some(32));
    }

    #[test]
    fn test_cpu_only_config() {
        let config = InferenceConfig::cpu_only();
        assert_eq!(config.n_gpu_layers, 0);
    }

    #[test]
    fn test_gpu_config() {
        let config = InferenceConfig::with_gpu(32);
        assert_eq!(config.n_gpu_layers, 32);
    }

    #[test]
    fn test_config_builder() {
        let config = InferenceConfig::default()
            .with_context_size(8192)
            .with_threads(8)
            .with_batch_size(256);

        assert_eq!(config.context_size, 8192);
        assert_eq!(config.n_threads, Some(8));
        assert_eq!(config.batch_size, 256);
    }

    #[test]
    fn test_isq_config() {
        let config = InferenceConfig::default().with_isq(IsqType::Q4K);
        assert!(config.enable_isq);
        assert!(matches!(config.isq_type, Some(IsqType::Q4K)));
    }

    #[test]
    fn test_config_validation() {
        let mut config = InferenceConfig::default();
        assert!(config.validate().is_ok());

        config.context_size = 0;
        assert!(config.validate().is_err());

        config.context_size = 512;
        config.batch_size = 1024;
        assert!(config.validate().is_err()); // batch_size > context_size

        config.batch_size = 0;
        assert!(config.validate().is_err());

        config.batch_size = 256;
        assert!(config.validate().is_ok());
    }
}
