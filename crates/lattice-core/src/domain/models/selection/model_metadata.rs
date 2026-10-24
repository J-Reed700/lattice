//! Catalog model metadata, its download URL and its resource checks.
use super::*;

/// Complete metadata for an AI model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Unique model identifier (e.g., "phi-3-mini-4k-instruct-q4")
    pub id: String,
    /// Human-readable name (e.g., "Phi-3 Mini")
    pub name: String,
    /// Model category (LLM, Embedding, OCR)
    pub category: ModelCategory,
    /// Detailed description of model capabilities
    pub description: String,
    /// Model size in gigabytes (on disk)
    pub size_gb: f64,
    /// Minimum RAM required to load model
    pub minimum_ram_gb: f64,
    /// Recommended RAM for good performance
    pub recommended_ram_gb: f64,
    /// Maximum context length in tokens
    pub context_length: u32,
    /// Performance characteristic
    pub performance_tier: PerformanceTier,
    /// Available quantization formats (e.g., Q4_K_M, Q5_K_M, F16)
    pub supported_quantizations: Vec<String>,
    /// Model capabilities (e.g., "chat", "code", "reasoning")
    pub capabilities: Vec<String>,
    /// Direct download URL for the model's repository or file.
    ///
    /// The shipped catalog sets this for every entry; `build_download_url()`
    /// prefers `model_id` + `default_filename` when both are present and
    /// resolves to this URL otherwise.
    pub download_url: Option<String>,
    /// License (e.g., "MIT", "Apache-2.0")
    pub license: String,
    /// Whether this model requires authentication to download
    #[serde(default)]
    pub requires_auth: bool,
    /// HuggingFace model ID (e.g., "microsoft/Phi-3-mini-4k-instruct-gguf")
    /// Used to construct download URLs dynamically
    #[serde(default)]
    pub model_id: Option<String>,
    /// Default GGUF filename for this model (e.g., "Phi-3-mini-4k-instruct-q4.gguf")
    /// Used with model_id to build download URLs
    #[serde(default)]
    pub default_filename: Option<String>,
    /// Files required for this model (for multi-file models like BGE-M3 ONNX)
    /// For single-file models, this will contain one ModelFile
    /// If empty, the model is fetched as a single file from `download_url`.
    #[serde(default)]
    pub files: Vec<crate::domain::models::metadata::ModelFileMetadata>,
    /// Total size across all files in bytes
    /// Used for disk space calculations and progress tracking
    #[serde(default)]
    pub total_size_bytes: u64,
    /// Output dimension for embedding models (e.g., 384, 768, 1024).
    /// None for non-embedding models (LLM, OCR).
    /// Used to filter catalog models to those compatible with the app's vector index.
    #[serde(default)]
    pub embedding_dimensions: Option<usize>,
    /// For embedding models: whether the bundled llama-server can serve this
    /// architecture. `None` for non-embedding models. Threaded through
    /// from `ExternalModelMetadata` so the catalog UI can badge incompatible
    /// rows.
    #[serde(default)]
    pub embedding_compatibility: Option<EmbeddingCompatibility>,
    /// On-disk storage format. Drives which loader path is used
    /// at load time and gates which loader-side pre-flight checks run
    /// (GGUF arch validation vs safetensors RAM check). Defaults to
    /// GGUF for backwards compat with existing catalog entries.
    #[serde(default)]
    pub format: ModelFormat,
}

impl ModelMetadata {
    /// Build HuggingFace download URL from model_id and default_filename.
    pub fn build_download_url(&self) -> Result<String> {
        if let (Some(model_id), Some(filename)) = (&self.model_id, &self.default_filename) {
            if !model_id.contains('/') || model_id.contains("..") {
                return Err(AppError::InvalidInput(format!(
                    "Invalid model_id format: {}",
                    model_id
                )));
            }

            if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
                return Err(AppError::InvalidInput(format!(
                    "Invalid filename contains path separators: {}",
                    filename
                )));
            }

            // URL-encode both model_id and filename to handle special characters
            // Need to encode each path segment separately to preserve the /
            let parts: Vec<&str> = model_id.split('/').collect();
            let encoded_org = parts
                .first()
                .map(|s| urlencoding::encode(s))
                .ok_or_else(|| {
                    AppError::InvalidInput(
                        "Invalid model ID format: missing organization".to_string(),
                    )
                })?;
            let encoded_repo = parts
                .get(1)
                .map(|s| urlencoding::encode(s))
                .ok_or_else(|| {
                    AppError::InvalidInput(
                        "Invalid model ID format: missing repository".to_string(),
                    )
                })?;

            Ok(format!(
                "https://huggingface.co/{}/{}/resolve/main/{}",
                encoded_org,
                encoded_repo,
                urlencoding::encode(filename)
            ))
        } else {
            // Multi-file and repo-only entries carry no `default_filename`;
            // fall back to the catalog's direct URL.
            self.download_url.clone().ok_or_else(|| {
                AppError::NotFound("No download URL available for this model".to_string())
            })
        }
    }

    /// Check if model fits in available RAM.
    pub fn fits_in_ram(&self, total_ram_gb: f64) -> bool {
        total_ram_gb >= self.minimum_ram_gb
    }

    /// Check if model fits on disk.
    pub fn fits_on_disk(&self, available_disk_gb: f64) -> bool {
        available_disk_gb >= self.size_gb
    }

    /// Estimate if model will perform well with given RAM.
    pub fn has_recommended_ram(&self, total_ram_gb: f64) -> bool {
        total_ram_gb >= self.recommended_ram_gb
    }
}
