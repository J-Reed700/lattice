//! # Hugging Face Model Catalog Adapter
//!
//! Infrastructure adapter for accessing Hugging Face Model Hub API.
//!
//! ## Architecture
//!
//! Implements `ModelCatalogPort` for real-time model discovery via HTTP API.
//!
//! ## Features
//!
//! - **HTTP Client**: reqwest with connection pooling
//! - **Rate Limiting**: 500ms minimum interval between requests
//! - **Error Handling**: Network errors, API errors, timeout handling
//! - **Graceful Degradation**: Returns empty results on errors (doesn't crash)
//!
//! ## API Endpoint
//!
//! - Base URL: `https://huggingface.co/api/models`
//! - Query params: `search`, `limit`, `filter` (e.g., task:text-generation)
//! - No authentication required for public models
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::infrastructure::huggingface_adapter::HuggingFaceAdapter;
//! use vault_desktop::application::ports::ModelCatalogPort;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let adapter = HuggingFaceAdapter::new();
//!     let models = adapter.search_models("llama", 10).await?;
//!
//!     for model in models {
//!         println!("{}: {}", model.id, model.name);
//!     }
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use parking_lot::Mutex;
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::application::ports::{ExternalModelMetadata, ModelCatalogPort};
use crate::shared::error::AppError;
use crate::shared::utils::reqwest_client_builder;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct HuggingFaceSibling {
    #[serde(rename = "rfilename")]
    filename: String,
    #[serde(default)]
    size: Option<u64>,
}

/// Hugging Face API response for model search.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct HuggingFaceModel {
    /// Model ID (e.g., "meta-llama/Llama-3.2-7B-Instruct")
    #[serde(rename = "modelId")]
    model_id: Option<String>,
    /// Alternative ID field
    id: Option<String>,
    /// Model name/display name
    #[serde(default)]
    name: Option<String>,
    /// Model author/organization
    #[serde(default)]
    author: Option<String>,
    /// Tags (e.g., ["text-generation", "llama"])
    #[serde(default)]
    tags: Vec<String>,
    /// Number of downloads
    #[serde(default)]
    downloads: u64,
    /// Last modified timestamp
    #[serde(rename = "lastModified")]
    last_modified: Option<String>,
    /// License
    #[serde(default)]
    license: Option<String>,
    /// Primary library (e.g., "transformers", "gguf")
    #[serde(rename = "library_name", default)]
    library_name: Option<String>,
    /// Number of likes on Hugging Face
    #[serde(default)]
    likes: u64,
    /// Pipeline tag (primary task)
    #[serde(rename = "pipeline_tag")]
    pipeline_tag: Option<String>,
    /// Parsed model card metadata when available
    #[serde(rename = "cardData", default)]
    card_data: Option<serde_json::Value>,
    /// Whether this model is gated (requires authentication)
    #[serde(default, deserialize_with = "deserialize_gated_flag")]
    gated: Option<bool>,
    /// Repository files (available with `full=true` or model detail endpoint)
    #[serde(default)]
    siblings: Vec<HuggingFaceSibling>,
}

fn deserialize_gated_flag<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;

    let parsed = match value {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Bool(b)) => Some(b),
        Some(serde_json::Value::Number(n)) => Some(n.as_i64().unwrap_or(0) != 0),
        Some(serde_json::Value::String(s)) => {
            let normalized = s.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "false" | "0" | "no" | "none" | "ungated" | "public" => Some(false),
                "true" | "1" | "yes" | "auto" | "manual" | "gated" => Some(true),
                // Treat unknown string states as gated for safety.
                _ => Some(true),
            }
        }
        Some(serde_json::Value::Array(_)) | Some(serde_json::Value::Object(_)) => None,
    };

    Ok(parsed)
}

impl HuggingFaceModel {
    fn resolve_repo_id(&self) -> String {
        self.model_id
            .clone()
            .or_else(|| self.id.clone())
            .unwrap_or_else(|| "unknown".into())
    }

    fn quantization_rank(filename: &str) -> u8 {
        let lower = filename.to_lowercase();
        if lower.contains("q4_k_m") {
            0
        } else if lower.contains("q4") {
            1
        } else if lower.contains("q5_k_m") {
            2
        } else if lower.contains("q5") {
            3
        } else if lower.contains("q8_0") {
            4
        } else {
            5
        }
    }

    /// Check if this model looks like an embedding/feature-extraction model.
    fn is_embedding_model(&self) -> bool {
        let tags_lower: Vec<String> = self.tags.iter().map(|t| t.to_lowercase()).collect();
        tags_lower.iter().any(|t| {
            t.contains("sentence-transformers")
                || t.contains("feature-extraction")
                || t.contains("embedding")
        }) || self.pipeline_tag.as_ref().is_some_and(|p| {
            p.eq_ignore_ascii_case("feature-extraction")
                || p.eq_ignore_ascii_case("sentence-similarity")
        })
    }

    /// Choose a preferred downloadable GGUF file from the repository.
    fn select_preferred_gguf_file(&self) -> Option<(String, Option<u64>)> {
        let mut candidates: Vec<&HuggingFaceSibling> = self
            .siblings
            .iter()
            .filter(|s| {
                let lower = s.filename.to_lowercase();
                lower.ends_with(".gguf")
                    && !s.filename.contains('/')
                    && !s.filename.contains('\\')
                    && !s.filename.contains("..")
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        candidates.sort_by(|a, b| {
            let rank_a = Self::quantization_rank(&a.filename);
            let rank_b = Self::quantization_rank(&b.filename);
            rank_a
                .cmp(&rank_b)
                .then(a.size.unwrap_or(u64::MAX).cmp(&b.size.unwrap_or(u64::MAX)))
        });

        let chosen = candidates.first().copied()?;
        Some((chosen.filename.clone(), chosen.size))
    }

    /// Choose a preferred ONNX file for embedding models.
    ///
    /// Embedding models require ONNX format with tokenizer.json. This method
    /// finds ONNX files in the repo siblings, preferring `model.onnx` or
    /// files in `onnx/` subdirectories.
    fn select_preferred_onnx_file(&self) -> Option<(String, Option<u64>)> {
        let mut candidates: Vec<&HuggingFaceSibling> = self
            .siblings
            .iter()
            .filter(|s| {
                let lower = s.filename.to_lowercase();
                lower.ends_with(".onnx") && !lower.ends_with(".onnx_data")
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Also verify tokenizer.json exists — required for ONNX embeddings
        let has_tokenizer = self
            .siblings
            .iter()
            .any(|s| s.filename == "tokenizer.json" || s.filename.ends_with("/tokenizer.json"));

        if !has_tokenizer {
            return None;
        }

        // Prefer "model.onnx" or "onnx/model.onnx" over other ONNX files
        candidates.sort_by(|a, b| {
            let a_is_model = a.filename.ends_with("model.onnx");
            let b_is_model = b.filename.ends_with("model.onnx");
            b_is_model
                .cmp(&a_is_model)
                .then(a.filename.len().cmp(&b.filename.len()))
        });

        let chosen = candidates.first().copied()?;
        Some((chosen.filename.clone(), chosen.size))
    }

    /// Choose a preferred downloadable file from the repository.
    ///
    /// For embedding models, prefers ONNX files (required by OnnxEmbeddingService),
    /// falling back to GGUF so the model is still discoverable (filtered later).
    /// For all other models, prefers GGUF files.
    fn select_preferred_file(&self) -> Option<(String, Option<u64>)> {
        if self.is_embedding_model() {
            self.select_preferred_onnx_file()
                .or_else(|| self.select_preferred_gguf_file())
        } else {
            self.select_preferred_gguf_file()
        }
    }

    fn compact_text(text: &str, max_chars: usize) -> String {
        let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.chars().count() <= max_chars {
            return collapsed;
        }

        let mut out = String::with_capacity(max_chars + 3);
        for (idx, ch) in collapsed.chars().enumerate() {
            if idx >= max_chars.saturating_sub(3) {
                break;
            }
            out.push(ch);
        }
        out.push_str("...");
        out
    }

    fn json_value_to_text(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Array(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .filter_map(Self::json_value_to_text)
                    .filter(|s| !s.trim().is_empty())
                    .collect();
                if parts.is_empty() {
                    None
                } else {
                    Some(parts.join(", "))
                }
            }
            serde_json::Value::Object(map) => map
                .get("name")
                .and_then(Self::json_value_to_text)
                .or_else(|| map.get("id").and_then(Self::json_value_to_text)),
            _ => None,
        }
    }

    fn card_data_text(&self, keys: &[&str]) -> Option<String> {
        let card = self.card_data.as_ref()?.as_object()?;
        for key in keys {
            if let Some(value) = card.get(*key) {
                if let Some(text) = Self::json_value_to_text(value) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(Self::compact_text(trimmed, 220));
                    }
                }
            }
        }
        None
    }

    fn human_task_name(task: &str) -> String {
        task.replace(['-', '_'], " ")
    }

    fn build_description(&self, repo_id: &str, has_gguf: bool) -> String {
        if let Some(summary) = self.card_data_text(&["summary", "description", "model_description"])
        {
            return summary;
        }

        let mut parts = Vec::new();

        if let Some(task) = &self.pipeline_tag {
            parts.push(format!("{} model", Self::human_task_name(task)));
        } else if has_gguf {
            parts.push("GGUF model".to_string());
        } else {
            parts.push("Foundation model".to_string());
        }

        if let Some(author) = &self.author {
            if !author.trim().is_empty() {
                parts.push(format!("by {}", author.trim()));
            }
        }

        if let Some(base_model) = self.card_data_text(&["base_model", "base_models"]) {
            parts.push(format!("base: {}", base_model));
        }

        if let Some(library) = &self.library_name {
            if !library.trim().is_empty() {
                parts.push(format!("library: {}", library.trim()));
            }
        }

        if self.likes > 0 {
            parts.push(format!("{} likes", self.likes));
        }

        if parts.is_empty() {
            return format!("Hugging Face model: {}", repo_id);
        }

        Self::compact_text(&parts.join(" • "), 220)
    }

    /// Convert to ExternalModelMetadata.
    fn to_external_metadata(&self) -> ExternalModelMetadata {
        let id = self.resolve_repo_id();

        let name = self.name.clone().unwrap_or_else(|| id.clone());
        let preferred_file = self.select_preferred_file();

        // Add pipeline_tag to tags if present
        let mut tags = self.tags.clone();
        if let Some(pipeline) = &self.pipeline_tag {
            if !tags.contains(pipeline) {
                tags.insert(0, pipeline.clone());
            }
        }
        let preferred_is_gguf = preferred_file
            .as_ref()
            .is_some_and(|(f, _)| f.to_lowercase().ends_with(".gguf"));
        if preferred_is_gguf && !tags.iter().any(|t| t.eq_ignore_ascii_case("gguf")) {
            tags.push("gguf".into());
        }
        let description = self.build_description(&id, preferred_is_gguf);

        // Detect Candle compatibility for embedding models from architecture
        // tags. Non-embedding models get None.
        let embedding_compatibility = if self.is_embedding_model() {
            Some(crate::features::embedding::compatibility::detect_from_tags(&tags))
        } else {
            None
        };

        ExternalModelMetadata {
            id: id.clone(),
            name,
            description,
            tags,
            downloads: self.downloads,
            likes: self.likes,
            download_url: Some(format!("https://huggingface.co/{}", id)),
            license: self.license.clone().unwrap_or_else(|| "unknown".into()),
            last_modified: self
                .last_modified
                .clone()
                .unwrap_or_else(|| "unknown".into()),
            gated: self.gated,
            preferred_filename: preferred_file.as_ref().map(|f| f.0.clone()),
            preferred_size_bytes: preferred_file.and_then(|f| f.1),
            embedding_compatibility,
        }
    }
}

/// Hugging Face Model Hub adapter.
///
/// Implements `ModelCatalogPort` by querying the Hugging Face API.
pub struct HuggingFaceAdapter {
    client: Client,
    last_request: Arc<Mutex<Option<Instant>>>,
    min_request_interval: Duration,
}

impl HuggingFaceAdapter {
    /// Create a new Hugging Face adapter.
    ///
    /// # Configuration
    /// - HTTP client with 10s timeout
    /// - Rate limiting: 500ms between requests
    pub fn new() -> Self {
        let client = reqwest_client_builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            last_request: Arc::new(Mutex::new(None)),
            min_request_interval: Duration::from_millis(500),
        }
    }

    /// Enforce rate limiting.
    ///
    /// Sleeps if necessary to maintain minimum interval between requests.
    async fn enforce_rate_limit(&self) {
        // Scoped locking: compute delay while holding lock, then release before await
        let delay_needed = {
            let mut guard = self.last_request.lock();

            if let Some(last_time) = *guard {
                let elapsed = last_time.elapsed();
                if elapsed < self.min_request_interval {
                    Some(self.min_request_interval - elapsed)
                } else {
                    *guard = Some(Instant::now());
                    None
                }
            } else {
                *guard = Some(Instant::now());
                None
            }
        }; // Guard dropped here

        // Safe: no guard held across await point
        if let Some(delay) = delay_needed {
            tokio::time::sleep(delay).await;
            // Update timestamp after sleep
            *self.last_request.lock() = Some(Instant::now());
        }
    }

    /// Search Hugging Face models via API.
    ///
    /// # Arguments
    /// - `query` - Search query
    /// - `limit` - Maximum number of results
    ///
    /// # Returns
    /// List of models or error.
    async fn search_api(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<HuggingFaceModel>, AppError> {
        self.enforce_rate_limit().await;

        let url = format!(
            "https://huggingface.co/api/models?search={}&limit={}&full=true&sort=downloads&direction=-1",
            urlencoding::encode(query),
            limit
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "RecallDesktop/1.0")
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Hugging Face API request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "Hugging Face API returned status: {}",
                response.status()
            )));
        }

        let models: Vec<HuggingFaceModel> = response.json().await.map_err(|e| {
            AppError::Network(format!("Failed to parse Hugging Face response: {}", e))
        })?;

        Ok(models)
    }

    /// Get a specific model by ID.
    ///
    /// # Arguments
    /// - `model_id` - Model identifier (e.g., "meta-llama/Llama-3.2-7B")
    ///
    /// # Returns
    /// Model metadata or error.
    async fn get_model_api(&self, model_id: &str) -> Result<HuggingFaceModel, AppError> {
        self.enforce_rate_limit().await;

        let encoded_model_id = model_id
            .split('/')
            .map(urlencoding::encode)
            .collect::<Vec<_>>()
            .join("/");
        let url = format!(
            "https://huggingface.co/api/models/{}?full=true",
            encoded_model_id
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "RecallDesktop/1.0")
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Hugging Face API request failed: {}", e)))?;

        if response.status().as_u16() == 404 {
            return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
        }

        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "Hugging Face API returned status: {}",
                response.status()
            )));
        }

        let model: HuggingFaceModel = response.json().await.map_err(|e| {
            AppError::Network(format!("Failed to parse Hugging Face response: {}", e))
        })?;

        Ok(model)
    }
}

impl Default for HuggingFaceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelCatalogPort for HuggingFaceAdapter {
    async fn search_models(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ExternalModelMetadata>, AppError> {
        // Graceful degradation: return empty on error instead of crashing
        match self.search_api(query, limit).await {
            Ok(models) => Ok(models
                .into_iter()
                .map(|m| m.to_external_metadata())
                .collect()),
            Err(e) => {
                tracing::warn!(
                    "Hugging Face API search failed: {}. Returning empty results.",
                    e
                );
                Ok(Vec::new())
            }
        }
    }

    async fn get_model_by_id(
        &self,
        model_id: &str,
    ) -> Result<Option<ExternalModelMetadata>, AppError> {
        // Graceful degradation: return None on error instead of crashing
        match self.get_model_api(model_id).await {
            Ok(model) => Ok(Some(model.to_external_metadata())),
            Err(AppError::NotFound(_)) => Ok(None),
            Err(e) => {
                tracing::warn!("Hugging Face API get_model failed: {}. Returning None.", e);
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_huggingface_adapter_creation() {
        let adapter = HuggingFaceAdapter::new();
        assert!(adapter.last_request.lock().is_none());
    }

    #[test]
    fn test_huggingface_model_to_external_metadata() {
        let hf_model = HuggingFaceModel {
            model_id: Some("test/model".into()),
            id: None,
            name: Some("Test Model".into()),
            author: None,
            tags: vec!["text-generation".into()],
            downloads: 1000,
            last_modified: Some("2024-01-01T00:00:00Z".into()),
            license: Some("apache-2.0".into()),
            library_name: None,
            likes: 0,
            pipeline_tag: None,
            card_data: None,
            gated: None,
            siblings: vec![],
        };

        let external = hf_model.to_external_metadata();

        assert_eq!(external.id, "test/model");
        assert_eq!(external.name, "Test Model");
        assert_eq!(external.tags, vec!["text-generation"]);
        assert_eq!(external.downloads, 1000);
        assert_eq!(external.license, "apache-2.0");
    }

    #[test]
    fn test_huggingface_model_fallbacks() {
        let hf_model = HuggingFaceModel {
            model_id: None,
            id: None,
            name: None,
            author: None,
            tags: vec![],
            downloads: 0,
            last_modified: None,
            gated: None,
            license: None,
            library_name: None,
            likes: 0,
            pipeline_tag: None,
            card_data: None,
            siblings: vec![],
        };

        let external = hf_model.to_external_metadata();

        assert_eq!(external.id, "unknown");
        assert_eq!(external.name, "unknown");
        assert_eq!(external.license, "unknown");
        assert_eq!(external.last_modified, "unknown");
    }

    #[test]
    fn test_huggingface_model_deserializes_string_gated() {
        let json = r#"{
            "modelId":"test/model",
            "id":"test/model",
            "name":"Test Model",
            "tags":["text-generation"],
            "downloads":42,
            "lastModified":"2024-01-01T00:00:00Z",
            "license":"apache-2.0",
            "pipeline_tag":"text-generation",
            "gated":"auto",
            "siblings":[]
        }"#;

        let model: HuggingFaceModel = serde_json::from_str(json).expect("deserialize model");
        assert_eq!(model.gated, Some(true));
    }

    #[test]
    fn test_huggingface_model_uses_card_data_summary_for_description() {
        let hf_model = HuggingFaceModel {
            model_id: Some("org/model".into()),
            id: None,
            name: Some("Model".into()),
            author: Some("org".into()),
            tags: vec!["text-generation".into()],
            downloads: 1234,
            last_modified: Some("2024-01-01T00:00:00Z".into()),
            license: Some("apache-2.0".into()),
            library_name: Some("gguf".into()),
            likes: 99,
            pipeline_tag: Some("text-generation".into()),
            card_data: Some(serde_json::json!({
                "summary": "A compact instruction-tuned model for chat and coding."
            })),
            gated: None,
            siblings: vec![],
        };

        let external = hf_model.to_external_metadata();
        assert_eq!(
            external.description,
            "A compact instruction-tuned model for chat and coding."
        );
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let adapter = HuggingFaceAdapter::new();

        let start = Instant::now();
        adapter.enforce_rate_limit().await;
        adapter.enforce_rate_limit().await;
        let elapsed = start.elapsed();

        // Second call should wait ~500ms
        assert!(elapsed >= Duration::from_millis(450));
    }

    // Integration tests (requires network, mark with #[ignore])
    #[tokio::test]
    #[ignore = "Requires network access"]
    async fn test_search_models_integration() {
        let adapter = HuggingFaceAdapter::new();
        let results = adapter.search_models("phi-3", 5).await.unwrap();

        assert!(!results.is_empty());
        for result in results {
            assert!(!result.id.is_empty());
            assert!(!result.name.is_empty());
        }
    }

    #[tokio::test]
    #[ignore = "Requires network access"]
    async fn test_get_model_by_id_integration() {
        let adapter = HuggingFaceAdapter::new();
        let model = adapter
            .get_model_by_id("microsoft/phi-3-mini-4k-instruct")
            .await
            .unwrap();

        assert!(model.is_some());
        let model = model.unwrap();
        assert!(model.id.contains("phi-3"));
    }

    #[tokio::test]
    #[ignore = "Requires network access"]
    async fn test_get_model_by_id_not_found() {
        let adapter = HuggingFaceAdapter::new();
        let model = adapter
            .get_model_by_id("nonexistent/model-that-does-not-exist-12345")
            .await
            .unwrap();

        assert!(model.is_none());
    }
}
