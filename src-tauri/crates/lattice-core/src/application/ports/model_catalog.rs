//! # External model catalog port
//!
//! Port for accessing external model catalogs (e.g., Hugging Face).
//!
//! ## Purpose
//!
//! Provides an abstraction for querying external model repositories
//! to dynamically discover models beyond the curated catalog.
//!
//! ## Implementations
//!
//! - **Production**: HTTP client to Hugging Face API
//! - **Mock**: Configurable mock for testing
//!
//! ## Example
//!
//! ```rust,no_run
//! use crate::application::ports::ModelCatalogPort;
//!
//! async fn search_models(catalog: &dyn ModelCatalogPort) -> Result<()> {
//!     let models = catalog.search_models("llama", 10).await?;
//!     for model in models {
//!         println!("{}: {}", model.id, model.name);
//!     }
//!     Ok(())
//! }
//! ```

use crate::domain::models::selection::{ModelCategory, ModelMetadata, PerformanceTier};
use crate::shared::error::AppError;
use async_trait::async_trait;

mod mock;

pub use mock::MockModelCatalogPort;

/// External model metadata from API.
///
/// This DTO contains raw metadata from external sources (e.g., Hugging Face)
/// which needs to be mapped to our domain ModelMetadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExternalModelMetadata {
    /// Model identifier (e.g., "meta-llama/Llama-3.2-7B-Instruct")
    pub id: String,
    /// Model name (e.g., "Llama 3.2 7B Instruct")
    pub name: String,
    /// Model description/README excerpt
    pub description: String,
    /// Tags from model repository (e.g., ["text-generation", "llama"])
    pub tags: Vec<String>,
    /// Number of downloads (popularity metric)
    pub downloads: u64,
    /// Number of likes from upstream provider (popularity/quality signal)
    #[serde(default)]
    pub likes: u64,
    /// Download URL if available
    pub download_url: Option<String>,
    /// License identifier (e.g., "llama-3.2", "apache-2.0")
    pub license: String,
    /// Last modified timestamp (ISO 8601)
    pub last_modified: String,
    /// Whether this model is gated (requires authentication)
    pub gated: Option<bool>,
    /// Preferred downloadable filename (typically GGUF) resolved from provider metadata.
    #[serde(default)]
    pub preferred_filename: Option<String>,
    /// Preferred file size in bytes.
    #[serde(default)]
    pub preferred_size_bytes: Option<u64>,
    /// For embedding models: whether the local Candle inference path can load
    /// this architecture. `None` for non-embedding models. Catalog browsers
    /// surface this so users see a "Coming soon" badge instead of starting a
    /// download that will fail.
    #[serde(default)]
    pub embedding_compatibility: Option<crate::domain::models::selection::EmbeddingCompatibility>,
}

impl ExternalModelMetadata {
    /// Build a stable internal ID for Hugging Face download targets.
    ///
    /// Format: `hf.<base64url(repo_id)>.<base64url(filename)>`
    pub fn encode_hf_download_id(repo_id: &str, filename: &str) -> String {
        let repo = Self::encode_base64_url(repo_id);
        let file = Self::encode_base64_url(filename);
        format!("hf.{}.{}", repo, file)
    }

    /// Decode an internal Hugging Face download ID back into (repo_id, filename).
    pub fn decode_hf_download_id(internal_id: &str) -> Option<(String, String)> {
        let mut parts = internal_id.splitn(3, '.');
        if parts.next()? != "hf" {
            return None;
        }

        let repo_part = parts.next()?;
        let file_part = parts.next()?;
        let repo = Self::decode_base64_url(repo_part).ok()?;
        let file = Self::decode_base64_url(file_part).ok()?;

        if repo.is_empty() || file.is_empty() {
            return None;
        }

        Some((repo, file))
    }

    /// Base64URL encode without padding (RFC 4648).
    fn encode_base64_url(input: &str) -> String {
        fn encode_value(value: u32) -> char {
            match value {
                0..=25 => (b'A' + value as u8) as char,
                26..=51 => (b'a' + (value as u8 - 26)) as char,
                52..=61 => (b'0' + (value as u8 - 52)) as char,
                62 => '-',
                63 => '_',
                _ => unreachable!("base64url value must be within 0..=63"),
            }
        }

        let bytes = input.as_bytes();
        let mut output = String::with_capacity((bytes.len() * 4).div_ceil(3));

        for chunk in bytes.chunks(3) {
            match *chunk {
                [b0, b1, b2] => {
                    let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
                    output.push(encode_value((n >> 18) & 0x3f));
                    output.push(encode_value((n >> 12) & 0x3f));
                    output.push(encode_value((n >> 6) & 0x3f));
                    output.push(encode_value(n & 0x3f));
                }
                [b0, b1] => {
                    let n = ((b0 as u32) << 16) | ((b1 as u32) << 8);
                    output.push(encode_value((n >> 18) & 0x3f));
                    output.push(encode_value((n >> 12) & 0x3f));
                    output.push(encode_value((n >> 6) & 0x3f));
                }
                [b0] => {
                    let n = (b0 as u32) << 16;
                    output.push(encode_value((n >> 18) & 0x3f));
                    output.push(encode_value((n >> 12) & 0x3f));
                }
                _ => {}
            }
        }

        output
    }

    /// Base64URL decode without padding.
    fn decode_base64_url(input: &str) -> Result<String, AppError> {
        fn decode_char(c: u8) -> Option<u8> {
            match c {
                b'A'..=b'Z' => Some(c - b'A'),
                b'a'..=b'z' => Some(c - b'a' + 26),
                b'0'..=b'9' => Some(c - b'0' + 52),
                b'-' => Some(62),
                b'_' => Some(63),
                _ => None,
            }
        }

        let mut buffer: u32 = 0;
        let mut bits: u8 = 0;
        let mut out = Vec::with_capacity((input.len() * 3) / 4);

        for c in input.bytes() {
            let value = decode_char(c)
                .ok_or_else(|| AppError::Parsing(format!("Invalid base64url char: {}", c)))?;
            buffer = (buffer << 6) | (value as u32);
            bits += 6;

            while bits >= 8 {
                bits -= 8;
                out.push(((buffer >> bits) & 0xff) as u8);
            }
        }

        if bits > 0 {
            let mask = (1u32 << bits) - 1;
            if (buffer & mask) != 0 {
                return Err(AppError::Parsing(
                    "Invalid trailing bits in base64url payload".to_string(),
                ));
            }
        }

        String::from_utf8(out)
            .map_err(|e| AppError::Parsing(format!("Invalid UTF-8 payload: {}", e)))
    }

    /// Sanitize arbitrary model identifiers into safe path/DB IDs.
    fn sanitize_model_id(raw: &str) -> String {
        let mut id = String::with_capacity(raw.len());
        for ch in raw.chars() {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                id.push(ch);
            } else {
                id.push('-');
            }
        }

        let collapsed = id
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        if collapsed.is_empty() {
            "model".to_string()
        } else {
            collapsed
        }
    }

    /// Map external metadata to domain ModelMetadata.
    ///
    /// This performs inference and estimation:
    /// - Category: Inferred from tags
    /// - Size: Estimated from model name or defaults
    /// - RAM requirements: Heuristic based on estimated size
    /// - Quantization: Extracted from model ID/tags
    ///
    /// # Returns
    /// Domain ModelMetadata or error if mapping fails.
    pub fn to_domain_model(&self) -> Result<ModelMetadata, AppError> {
        // Infer category from tags
        let category = self.infer_category()?;

        // Estimate model size from name or use defaults
        let size_gb = if let Some(size_bytes) = self.preferred_size_bytes {
            size_bytes as f64 / 1_000_000_000.0
        } else if self.preferred_filename.is_some() {
            // Parameter count cannot tell us the size of a selected quantization.
            0.0
        } else {
            self.estimate_size_gb()
        };

        // Heuristic RAM requirements
        let minimum_ram_gb = if size_gb > 0.0 {
            (size_gb * 1.5).max(4.0)
        } else {
            0.0
        };
        let recommended_ram_gb = if size_gb > 0.0 {
            (size_gb * 2.0).max(8.0)
        } else {
            0.0
        };

        let context_length = self.extract_context_length();

        // Infer performance tier from size
        let performance_tier = if size_gb < 2.0 {
            PerformanceTier::Fast
        } else if size_gb < 8.0 {
            PerformanceTier::Balanced
        } else {
            PerformanceTier::Accurate
        };

        let supported_quantizations = self.extract_quantizations();

        let capabilities = self.extract_capabilities();

        let is_hf_repo_id = self.id.contains('/');
        let model_id = if is_hf_repo_id {
            Some(self.id.clone())
        } else {
            None
        };

        let default_filename = self.preferred_filename.clone();
        let internal_id = if let (Some(repo_id), Some(filename)) = (&model_id, &default_filename) {
            Self::encode_hf_download_id(repo_id, filename)
        } else {
            Self::sanitize_model_id(&self.id)
        };

        let resolved_download_url = self.download_url.clone().or_else(|| {
            model_id
                .as_ref()
                .map(|repo_id| format!("https://huggingface.co/{}", repo_id))
        });

        Ok(ModelMetadata {
            id: internal_id,
            name: self.name.clone(),
            category,
            description: self.description.clone(),
            size_gb,
            minimum_ram_gb,
            recommended_ram_gb,
            context_length,
            performance_tier,
            supported_quantizations,
            capabilities,
            download_url: resolved_download_url,
            license: self.license.clone(),
            requires_auth: self.gated.unwrap_or(false),
            model_id,
            default_filename,
            files: vec![],
            total_size_bytes: self
                .preferred_size_bytes
                .unwrap_or((size_gb * 1_000_000_000.0) as u64),
            embedding_dimensions: None,
            embedding_compatibility: self.embedding_compatibility.clone(),
            // Default to GGUF here. Catalog DTOs from external sources
            // don't carry a format field today; the curated entries set
            // it explicitly. Detection at download time can override.
            format: crate::domain::models::selection::ModelFormat::Gguf,
        })
    }

    /// Infer model category from tags.
    fn infer_category(&self) -> Result<ModelCategory, AppError> {
        let tags_lower: Vec<String> = self.tags.iter().map(|t| t.to_lowercase()).collect();

        // Embedding indicators take precedence. Decoder-backed embedders such
        // as Qwen3 legitimately carry both `text-generation` (their base
        // architecture) and `feature-extraction` / `sentence-transformers`
        // (their actual task). Classifying the architecture tag first hides
        // those models from the embedding catalog.
        if tags_lower.iter().any(|t| {
            t.contains("sentence-transformers")
                || t.contains("feature-extraction")
                || t.contains("embedding")
        }) {
            return Ok(ModelCategory::Embedding);
        }

        // LLM indicators
        if tags_lower.iter().any(|t| {
            t.contains("text-generation")
                || t.contains("llm")
                || t.contains("causal-lm")
                || t.contains("conversational")
        }) {
            return Ok(ModelCategory::LLM);
        }

        // OCR indicators
        if tags_lower.iter().any(|t| {
            t.contains("ocr")
                || t.contains("vision")
                || t.contains("image-text-to-text")
                || t.contains("document-question-answering")
        }) {
            return Ok(ModelCategory::OCR);
        }

        // Default to LLM if unclear
        Ok(ModelCategory::LLM)
    }

    /// Estimate model size from name/ID.
    fn estimate_size_gb(&self) -> f64 {
        let text = format!("{} {}", self.id, self.name).to_lowercase();

        // Embedding models vary widely in size and we don't want to show a
        // misleading estimate.  Return 0.0 so the frontend can display "Unknown".
        if matches!(self.infer_category(), Ok(ModelCategory::Embedding))
            || text.contains("embed")
            || self.tags.iter().any(|t| {
                let tag = t.to_lowercase();
                tag.contains("embedding")
                    || tag.contains("feature-extraction")
                    || tag.contains("sentence-transformers")
            })
        {
            return 0.0;
        }

        // OCR/vision models tend to sit between embedding and LLM sizes.
        if matches!(self.infer_category(), Ok(ModelCategory::OCR)) {
            return 1.5;
        }

        if let Some(size) = Self::extract_param_count(&text) {
            return Self::estimate_size_from_params(size);
        }

        // Pattern: "q4", "q5", "q8" (quantization)
        if text.contains("q4") || text.contains("gguf") {
            // Q4 quantized models
            if text.contains("7b") {
                return 4.5;
            } else if text.contains("13b") {
                return 8.0;
            } else if text.contains("3b") || text.contains("3.8b") {
                return 2.3;
            }
        }

        // Default fallback
        5.0
    }

    /// Extract parameter count from text (e.g., "7b" -> 7.0).
    fn extract_param_count(text: &str) -> Option<f64> {
        // Patterns: "7b", "13b", "70b", "1.1b", "3.8b"
        let re = regex::Regex::new(r"(\d+\.?\d*)\s*b\b").ok()?;
        let captures = re.captures(text)?;
        captures.get(1)?.as_str().parse().ok()
    }

    /// Estimate size in GB from parameter count (billions).
    fn estimate_size_from_params(params_b: f64) -> f64 {
        // Heuristic: Q4 quantization ~0.6 GB per billion parameters
        // F16: ~2 GB per billion, Q8: ~1 GB per billion
        (params_b * 0.6).max(0.5)
    }

    /// Extract context length from name/description.
    fn extract_context_length(&self) -> u32 {
        let text = format!("{} {}", self.name, self.description).to_lowercase();

        // Pattern: "4k", "8k", "32k", "128k"
        if text.contains("128k") || text.contains("131k") {
            131072
        } else if text.contains("32k") {
            32768
        } else if text.contains("16k") {
            16384
        } else if text.contains("8k") {
            8192
        } else if text.contains("4k") {
            4096
        } else if text.contains("2k") {
            2048
        } else {
            // Default
            4096
        }
    }

    /// The selected artifact is authoritative; repository tags can describe
    /// other variants. An unlabelled GGUF is not necessarily Q4 (or quantized).
    fn extract_quantizations(&self) -> Vec<String> {
        use std::sync::LazyLock;
        static PRECISION: LazyLock<Option<regex::Regex>> = LazyLock::new(|| {
            regex::Regex::new(
                r"(?i)(?:^|[.\-_ ])((?:UD-)?(?:IQ[1-8]|Q[1-8]|TQ[12])(?:_[A-Z0-9]+)*|BF16|F16|F32|FP16|FP32|MXFP4)(?:[.\- ]|$)",
            )
            .ok()
        });
        let text = self
            .preferred_filename
            .clone()
            .unwrap_or_else(|| format!("{} {} {}", self.id, self.name, self.tags.join(" ")));
        PRECISION
            .as_ref()
            .and_then(|pattern| pattern.captures(&text))
            .map(|capture| vec![capture[1].to_ascii_uppercase()])
            .unwrap_or_default()
    }

    /// Extract capabilities from tags.
    fn extract_capabilities(&self) -> Vec<String> {
        let mut capabilities = Vec::new();

        // First, infer capabilities from category
        if let Ok(category) = self.infer_category() {
            match category {
                ModelCategory::LLM => {
                    capabilities.push("chat".into());
                }
                ModelCategory::Embedding => {
                    capabilities.push("embedding".into());
                }
                ModelCategory::OCR => {
                    capabilities.push("vision".into());
                }
                ModelCategory::Transcription => {
                    capabilities.push("transcription".into());
                }
            }
        }

        // Then add specific capabilities from tags
        for tag in &self.tags {
            let tag_lower = tag.to_lowercase();
            if tag_lower.contains("chat") || tag_lower.contains("conversational") {
                capabilities.push("chat".into());
            }
            if tag_lower.contains("code") {
                capabilities.push("code".into());
            }
            if tag_lower.contains("reasoning") {
                capabilities.push("reasoning".into());
            }
            if tag_lower.contains("multilingual") {
                capabilities.push("multilingual".into());
            }
            if tag_lower.contains("embedding") {
                capabilities.push("embedding".into());
            }
            if tag_lower.contains("retrieval") {
                capabilities.push("retrieval".into());
            }
            if tag_lower.contains("ocr") || tag_lower.contains("vision") {
                capabilities.push("vision".into());
            }
        }

        if capabilities.is_empty() {
            capabilities.push("general".into());
        }

        capabilities.dedup();
        capabilities
    }
}

/// Port for accessing external model catalogs.
///
/// Provides dynamic model discovery from external repositories.
#[async_trait]
pub trait ModelCatalogPort: Send + Sync {
    /// Enumerate actual standalone download artifacts, not advertised formats.
    /// Adapters without a file listing can only offer their known default.
    async fn get_model_variants(
        &self,
        model_id: &str,
    ) -> Result<Vec<ExternalModelMetadata>, AppError> {
        Ok(self.get_model_by_id(model_id).await?.into_iter().collect())
    }

    /// Search for models by query.
    ///
    /// # Arguments
    /// - `query` - Search query (model name, keywords)
    /// - `limit` - Maximum number of results
    ///
    /// # Returns
    /// - `Ok(Vec<ExternalModelMetadata>)` - List of matching models
    /// - `Err(AppError)` - Network error, API error, or parsing error
    async fn search_models(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ExternalModelMetadata>, AppError>;

    /// Get model by ID.
    ///
    /// # Arguments
    /// - `model_id` - Model identifier (e.g., "meta-llama/Llama-3.2-7B")
    ///
    /// # Returns
    /// - `Ok(Some(ExternalModelMetadata))` - Model found
    /// - `Ok(None)` - Model not found
    /// - `Err(AppError)` - Network error or API error
    async fn get_model_by_id(
        &self,
        model_id: &str,
    ) -> Result<Option<ExternalModelMetadata>, AppError>;
}

#[cfg(test)]
mod tests;
