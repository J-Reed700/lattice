//! # Model Catalog Port (Phase 2: External Catalogs)
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

use crate::features::model_management::domain::{ModelCategory, ModelMetadata, PerformanceTier};
use crate::shared::error::AppError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

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
    pub embedding_compatibility:
        Option<crate::features::embedding::compatibility::EmbeddingCompatibility>,
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
    fn decode_base64_url(input: &str) -> Result<String, String> {
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
            let value = decode_char(c).ok_or_else(|| format!("Invalid base64url char: {}", c))?;
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
                return Err("Invalid trailing bits in base64url payload".into());
            }
        }

        String::from_utf8(out).map_err(|e| format!("Invalid UTF-8 payload: {}", e))
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
    pub fn to_domain_model(&self) -> Result<ModelMetadata, String> {
        // Infer category from tags
        let category = self.infer_category()?;

        // Estimate model size from name or use defaults
        let size_gb = if let Some(size_bytes) = self.preferred_size_bytes {
            (size_bytes as f64 / 1_000_000_000.0).max(0.1)
        } else {
            self.estimate_size_gb()
        };

        // Heuristic RAM requirements
        let minimum_ram_gb = (size_gb * 1.5).max(4.0);
        let recommended_ram_gb = (size_gb * 2.0).max(8.0);

        // Extract context length from name/description or default
        let context_length = self.extract_context_length();

        // Infer performance tier from size
        let performance_tier = if size_gb < 2.0 {
            PerformanceTier::Fast
        } else if size_gb < 8.0 {
            PerformanceTier::Balanced
        } else {
            PerformanceTier::Accurate
        };

        // Extract quantization formats
        let supported_quantizations = self.extract_quantizations();

        // Extract capabilities from tags
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
            format: crate::llm::models::ModelFormat::Gguf,
        })
    }

    /// Infer model category from tags.
    fn infer_category(&self) -> Result<ModelCategory, String> {
        let tags_lower: Vec<String> = self.tags.iter().map(|t| t.to_lowercase()).collect();

        // LLM indicators
        if tags_lower.iter().any(|t| {
            t.contains("text-generation")
                || t.contains("llm")
                || t.contains("causal-lm")
                || t.contains("conversational")
        }) {
            return Ok(ModelCategory::LLM);
        }

        // Embedding indicators
        if tags_lower.iter().any(|t| {
            t.contains("sentence-transformers")
                || t.contains("feature-extraction")
                || t.contains("embedding")
        }) {
            return Ok(ModelCategory::Embedding);
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

        // Extract size hints from text
        // Pattern: "7b", "13b", "70b" (billions of parameters)
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

    /// Extract quantization formats from ID/tags.
    fn extract_quantizations(&self) -> Vec<String> {
        let text = format!(
            "{} {} {} {}",
            self.id,
            self.name,
            self.tags.join(" "),
            self.preferred_filename.clone().unwrap_or_default()
        )
        .to_lowercase();
        let mut quantizations = Vec::new();

        if text.contains("q4") || text.contains("q4_k_m") {
            quantizations.push("Q4_K_M".into());
        }
        if text.contains("q5") {
            quantizations.push("Q5_K_M".into());
        }
        if text.contains("q8") {
            quantizations.push("Q8_0".into());
        }
        if text.contains("f16") {
            quantizations.push("F16".into());
        }
        if text.contains("gguf") && quantizations.is_empty() {
            // GGUF implies quantization
            quantizations.push("Q4_K_M".into());
        }

        if quantizations.is_empty() {
            quantizations.push("F16".into()); // Default
        }

        quantizations
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

/// Mock implementation for testing.
///
/// Provides configurable mock responses for testing
/// without making real API calls.
pub struct MockModelCatalogPort {
    models: Arc<Mutex<HashMap<String, ExternalModelMetadata>>>,
}

impl MockModelCatalogPort {
    /// Create a new mock catalog with default models.
    ///
    /// These models match the expectations of the test suite:
    /// - tinyllama: 1.1 GB (smallest)
    /// - phi-3-mini: 1.8 GB
    /// - llama-3.2-3b: 2.3 GB
    /// - llama-3.2-7b: 4.5 GB
    /// - qwen2.5-7b: 4.5 GB
    /// - mixtral-8x7b: 26.0 GB (largest)
    pub fn new() -> Self {
        let mut models = HashMap::new();

        // TinyLlama - smallest model (1.1 GB)
        models.insert(
            "tinyllama".into(),
            ExternalModelMetadata {
                id: "tinyllama".into(),
                name: "TinyLlama 1.1B".into(),
                description: "Compact 1.1B model for edge devices".into(),
                tags: vec!["text-generation".into(), "llama".into(), "chat".into()],
                downloads: 1000000,
                likes: 0,
                download_url: Some(
                    "https://huggingface.co/TinyLlama/TinyLlama-1.1B-Chat-v1.0".into(),
                ),
                license: "apache-2.0".into(),
                last_modified: "2024-01-01T00:00:00Z".into(),
                gated: None,
                preferred_filename: None,
                preferred_size_bytes: None,
                embedding_compatibility: None,
            },
        );

        // Phi-3 Mini - small model (1.8 GB) with code capabilities
        models.insert(
            "phi-3-mini".into(),
            ExternalModelMetadata {
                id: "phi-3-mini".into(),
                name: "Phi-3 Mini".into(),
                description: "Compact 3.8B model with strong reasoning and code capabilities"
                    .into(),
                tags: vec![
                    "text-generation".into(),
                    "phi".into(),
                    "code".into(),
                    "chat".into(),
                ],
                downloads: 800000,
                likes: 0,
                download_url: Some(
                    "https://huggingface.co/microsoft/phi-3-mini-4k-instruct".into(),
                ),
                license: "MIT".into(),
                last_modified: "2024-06-15T00:00:00Z".into(),
                gated: None,
                preferred_filename: None,
                preferred_size_bytes: None,
                embedding_compatibility: None,
            },
        );

        // Llama 3.2 3B - medium-small model (2.3 GB)
        models.insert(
            "llama-3.2-3b".into(),
            ExternalModelMetadata {
                id: "llama-3.2-3b".into(),
                name: "Llama 3.2 3B".into(),
                description: "Compact 3B instruction-tuned model with 4k context".into(),
                tags: vec!["text-generation".into(), "llama".into(), "chat".into()],
                downloads: 700000,
                likes: 0,
                download_url: Some(
                    "https://huggingface.co/meta-llama/Llama-3.2-3B-Instruct".into(),
                ),
                license: "llama-3.2".into(),
                last_modified: "2024-09-01T00:00:00Z".into(),
                gated: None,
                preferred_filename: None,
                preferred_size_bytes: None,
                embedding_compatibility: None,
            },
        );

        // Llama 3.2 7B - medium model (4.5 GB)
        models.insert(
            "llama-3.2-7b".into(),
            ExternalModelMetadata {
                id: "llama-3.2-7b".into(),
                name: "Llama 3.2 7B".into(),
                description: "Meta's 7B instruction-tuned model with improved reasoning".into(),
                tags: vec![
                    "text-generation".into(),
                    "llama".into(),
                    "conversational".into(),
                    "chat".into(),
                ],
                downloads: 900000,
                likes: 0,
                download_url: Some(
                    "https://huggingface.co/meta-llama/Llama-3.2-7B-Instruct".into(),
                ),
                license: "llama-3.2".into(),
                last_modified: "2024-09-01T00:00:00Z".into(),
                gated: None,
                preferred_filename: None,
                preferred_size_bytes: None,
                embedding_compatibility: None,
            },
        );

        // Qwen 2.5 7B - medium model with multilingual (4.5 GB)
        models.insert(
            "qwen2.5-7b".into(),
            ExternalModelMetadata {
                id: "qwen2.5-7b".into(),
                name: "Qwen 2.5 7B".into(),
                description: "Alibaba's 7B model with strong multilingual support".into(),
                tags: vec![
                    "text-generation".into(),
                    "multilingual".into(),
                    "chat".into(),
                ],
                downloads: 600000,
                likes: 0,
                download_url: Some("https://huggingface.co/Qwen/Qwen2.5-7B-Instruct".into()),
                license: "apache-2.0".into(),
                last_modified: "2024-10-01T00:00:00Z".into(),
                gated: None,
                preferred_filename: None,
                preferred_size_bytes: None,
                embedding_compatibility: None,
            },
        );

        // Mixtral 8x7B - largest model (26.0 GB)
        models.insert(
            "mixtral-8x7b".into(),
            ExternalModelMetadata {
                id: "mixtral-8x7b".into(),
                name: "Mixtral 8x7B".into(),
                description: "Mistral's mixture-of-experts 47B model with 8 experts".into(),
                tags: vec![
                    "text-generation".into(),
                    "mixtral".into(),
                    "chat".into(),
                    "code".into(),
                ],
                downloads: 500000,
                likes: 0,
                download_url: Some(
                    "https://huggingface.co/mistralai/Mixtral-8x7B-Instruct-v0.1".into(),
                ),
                license: "apache-2.0".into(),
                last_modified: "2024-02-01T00:00:00Z".into(),
                gated: None,
                preferred_filename: None,
                preferred_size_bytes: None,
                embedding_compatibility: None,
            },
        );

        Self {
            models: Arc::new(Mutex::new(models)),
        }
    }

    /// Create an empty mock for testing error cases.
    pub fn empty() -> Self {
        Self {
            models: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a model to the mock catalog.
    pub fn add_model(&self, model: ExternalModelMetadata) {
        self.models.lock().insert(model.id.clone(), model);
    }

    /// Clear all models.
    pub fn clear(&self) {
        self.models.lock().clear();
    }
}

impl Default for MockModelCatalogPort {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelCatalogPort for MockModelCatalogPort {
    async fn search_models(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ExternalModelMetadata>, AppError> {
        let models = self.models.lock();
        let query_lower = query.to_lowercase();

        let mut matching: Vec<ExternalModelMetadata> = models
            .values()
            .filter(|m| {
                m.name.to_lowercase().contains(&query_lower)
                    || m.id.to_lowercase().contains(&query_lower)
                    || m.description.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();

        // Sort by downloads (popularity)
        matching.sort_by(|a, b| b.downloads.cmp(&a.downloads));

        // Apply limit
        matching.truncate(limit);

        Ok(matching)
    }

    async fn get_model_by_id(
        &self,
        model_id: &str,
    ) -> Result<Option<ExternalModelMetadata>, AppError> {
        Ok(self.models.lock().get(model_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]

    async fn test_mock_catalog_search() {
        let catalog = MockModelCatalogPort::new();
        let results = catalog.search_models("llama", 10).await.unwrap();

        // Should find 3 models: tinyllama, llama-3.2-3b, llama-3.2-7b
        assert_eq!(results.len(), 3);
        assert!(results[0].id.contains("llama"));
    }

    #[tokio::test]
    async fn test_mock_catalog_get_by_id() {
        let catalog = MockModelCatalogPort::new();
        let model = catalog.get_model_by_id("llama-3.2-7b").await.unwrap();

        assert!(model.is_some());
        let model = model.unwrap();
        assert_eq!(model.name, "Llama 3.2 7B");
    }

    #[tokio::test]
    async fn test_mock_catalog_not_found() {
        let catalog = MockModelCatalogPort::new();
        let model = catalog.get_model_by_id("nonexistent").await.unwrap();

        assert!(model.is_none());
    }

    #[tokio::test]
    async fn test_mock_catalog_empty() {
        let catalog = MockModelCatalogPort::empty();
        let results = catalog.search_models("test", 10).await.unwrap();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_external_metadata_to_domain_llm() {
        let external = ExternalModelMetadata {
            id: "test/llama-7b-q4".into(),
            name: "Llama 7B Q4".into(),
            description: "Test LLM model".into(),
            tags: vec!["text-generation".into(), "llama".into()],
            downloads: 1000,
            likes: 0,
            download_url: None,
            license: "apache-2.0".into(),
            last_modified: "2024-01-01T00:00:00Z".into(),
            gated: None,
            preferred_filename: None,
            preferred_size_bytes: None,
            embedding_compatibility: None,
        };

        let domain = external.to_domain_model().unwrap();

        assert_eq!(domain.category, ModelCategory::LLM);
        assert!(domain.size_gb > 0.0);
        assert!(domain.minimum_ram_gb >= 4.0);
        assert!(domain.capabilities.contains(&"chat".into()));
    }

    #[test]
    fn test_external_metadata_to_domain_embedding() {
        let external = ExternalModelMetadata {
            id: "test/bge-m3".into(),
            name: "BGE-M3".into(),
            description: "Embedding model".into(),
            tags: vec!["sentence-transformers".into(), "feature-extraction".into()],
            downloads: 500,
            likes: 0,
            download_url: None,
            license: "MIT".into(),
            last_modified: "2024-01-01T00:00:00Z".into(),
            gated: None,
            preferred_filename: None,
            preferred_size_bytes: None,
            embedding_compatibility: None,
        };

        let domain = external.to_domain_model().unwrap();

        assert_eq!(domain.category, ModelCategory::Embedding);
        assert!(domain.capabilities.contains(&"embedding".into()));
    }

    #[test]
    fn test_infer_category_llm() {
        let external = ExternalModelMetadata {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            tags: vec!["text-generation".into()],
            downloads: 0,
            likes: 0,
            download_url: None,
            license: "".into(),
            last_modified: "".into(),
            gated: None,
            preferred_filename: None,
            preferred_size_bytes: None,
            embedding_compatibility: None,
        };

        let category = external.infer_category().unwrap();
        assert_eq!(category, ModelCategory::LLM);
    }

    #[test]
    fn test_estimate_size_7b_q4() {
        let external = ExternalModelMetadata {
            id: "test/model-7b-q4".into(),
            name: "Model 7B Q4".into(),
            description: "".into(),
            tags: vec![],
            downloads: 0,
            likes: 0,
            download_url: None,
            license: "".into(),
            last_modified: "".into(),
            gated: None,
            preferred_filename: None,
            preferred_size_bytes: None,
            embedding_compatibility: None,
        };

        let size = external.estimate_size_gb();
        assert!(size > 3.0 && size < 6.0);
    }

    #[test]
    fn test_extract_context_length() {
        let external = ExternalModelMetadata {
            id: "test".into(),
            name: "Model 32k context".into(),
            description: "".into(),
            tags: vec![],
            downloads: 0,
            likes: 0,
            download_url: None,
            license: "".into(),
            last_modified: "".into(),
            gated: None,
            preferred_filename: None,
            preferred_size_bytes: None,
            embedding_compatibility: None,
        };

        let context = external.extract_context_length();
        assert_eq!(context, 32768);
    }

    #[test]
    fn test_extract_quantizations() {
        let external = ExternalModelMetadata {
            id: "test/model-q4-gguf".into(),
            name: "Test".into(),
            description: "".into(),
            tags: vec![],
            downloads: 0,
            likes: 0,
            download_url: None,
            license: "".into(),
            last_modified: "".into(),
            gated: None,
            preferred_filename: None,
            preferred_size_bytes: None,
            embedding_compatibility: None,
        };

        let quants = external.extract_quantizations();
        assert!(quants.contains(&"Q4_K_M".into()));
    }

    #[test]
    fn test_extract_capabilities() {
        let external = ExternalModelMetadata {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            tags: vec!["chat".into(), "code".into(), "reasoning".into()],
            downloads: 0,
            likes: 0,
            download_url: None,
            license: "".into(),
            last_modified: "".into(),
            gated: None,
            preferred_filename: None,
            preferred_size_bytes: None,
            embedding_compatibility: None,
        };

        let caps = external.extract_capabilities();
        assert!(caps.contains(&"chat".into()));
        assert!(caps.contains(&"code".into()));
        assert!(caps.contains(&"reasoning".into()));
    }

    #[test]
    fn test_hf_id_round_trip() {
        let internal = ExternalModelMetadata::encode_hf_download_id(
            "meta-llama/Llama-3.2-3B-Instruct-GGUF",
            "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        );

        let decoded = ExternalModelMetadata::decode_hf_download_id(&internal).unwrap();
        assert_eq!(decoded.0, "meta-llama/Llama-3.2-3B-Instruct-GGUF");
        assert_eq!(decoded.1, "Llama-3.2-3B-Instruct-Q4_K_M.gguf");
    }

    #[test]
    fn test_to_domain_model_uses_internal_hf_id_when_downloadable() {
        let external = ExternalModelMetadata {
            id: "meta-llama/Llama-3.2-3B-Instruct-GGUF".into(),
            name: "Llama 3.2 3B Instruct".into(),
            description: "Test model".into(),
            tags: vec!["text-generation".into(), "gguf".into()],
            downloads: 100,
            likes: 0,
            download_url: None,
            license: "llama".into(),
            last_modified: "2024-01-01T00:00:00Z".into(),
            gated: Some(true),
            preferred_filename: Some("Llama-3.2-3B-Instruct-Q4_K_M.gguf".into()),
            preferred_size_bytes: Some(2_300_000_000),
            embedding_compatibility: None,
        };

        let domain = external.to_domain_model().unwrap();
        let decoded = ExternalModelMetadata::decode_hf_download_id(&domain.id).unwrap();

        assert_eq!(decoded.0, "meta-llama/Llama-3.2-3B-Instruct-GGUF");
        assert_eq!(decoded.1, "Llama-3.2-3B-Instruct-Q4_K_M.gguf");
        assert_eq!(
            domain.model_id,
            Some("meta-llama/Llama-3.2-3B-Instruct-GGUF".into())
        );
        assert_eq!(
            domain.default_filename,
            Some("Llama-3.2-3B-Instruct-Q4_K_M.gguf".into())
        );
        assert!(domain.requires_auth);
    }
}
