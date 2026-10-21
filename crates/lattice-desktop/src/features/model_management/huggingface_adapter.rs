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
//! use lattice::infrastructure::huggingface_adapter::HuggingFaceAdapter;
//! use lattice_core::application::ports::ModelCatalogPort;
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
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use std::time::Duration;

use lattice_core::application::ports::{ExternalModelMetadata, ModelCatalogPort};
use lattice_core::domain::models::selection::EmbeddingCompatibility;
use lattice_core::shared::error::AppError;
use lattice_core::shared::http::reqwest_client_builder;

mod hub_client;

pub use hub_client::HuggingFaceAdapter;

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
    /// Transformer configuration returned by the Hub when `full=true`.
    /// `model_type` is more reliable than repository tags for deciding which
    /// local embedding loader can open a model.
    #[serde(default)]
    config: Option<serde_json::Value>,
    /// GGUF header summary indexed by Hugging Face when available.
    #[serde(default)]
    gguf: Option<serde_json::Value>,
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
    fn is_standalone_gguf(filename: &str) -> bool {
        let lower = filename.to_ascii_lowercase();
        lower.ends_with(".gguf")
            && !filename.contains(['/', '\\'])
            && !filename.contains("..")
            && !lower.contains("mmproj")
            && !lower.contains("-of-")
            && !lower.contains("imatrix")
            && !lower.starts_with("ggml-vocab-")
            && !lower.starts_with("mtp-")
            && !lower.contains("-mtp-")
            && !lower.contains("_mtp_")
            && !lower.starts_with("dflash-")
            && !lower.contains("-dflash-")
            && !lower.contains("_dflash_")
            && !lower.starts_with("dspark-")
            && !lower.contains("-dspark-")
            && !lower.contains("_dspark_")
            && !lower.starts_with("eagle3-")
            && !lower.contains("-eagle3-")
            && !lower.contains("_eagle3_")
            && !lower.ends_with("-vision.gguf")
            && !lower.ends_with("_vision.gguf")
            && !lower.contains("pq2_0")
            && !lower.contains("ptq1_0")
    }

    fn gguf_architecture(&self) -> Option<&str> {
        self.gguf
            .as_ref()?
            .get("architecture")?
            .as_str()
            .filter(|arch| !arch.trim().is_empty())
    }

    /// Repositories whose checkpoints are Hadamard-rotated for the PrismML fork
    /// (`Ternary-Bonsai-2`, `PQ2_0` / `PTQ1_0`) cannot run on stock `llama.cpp`,
    /// even when they also publish an `F16.gguf` sibling.
    fn requires_fork_hadamard_runtime(&self) -> bool {
        let repo_lower = self.resolve_repo_id().to_ascii_lowercase();
        if repo_lower.contains("ternary-bonsai-2") {
            return true;
        }
        self.siblings.iter().any(|s| {
            let lower = s.filename.to_ascii_lowercase();
            lower.ends_with(".gguf") && (lower.contains("pq2_0") || lower.contains("ptq1_0"))
        })
    }

    /// PrismML's Ternary-Bonsai repos publish `-Q2_0.gguf` with a 128-element
    /// fork-only block size alongside `-Q2_0_g64.gguf` / `-Q2_g64.gguf` for
    /// mainline `llama.cpp` (where `GGML_TYPE_Q2_0` uses 64-element blocks).
    fn is_fork_g128_q2_0(&self, filename: &str) -> bool {
        let lower = filename.to_ascii_lowercase();
        let is_unsuffixed_q2_0 = lower.ends_with("-q2_0.gguf") || lower.ends_with("_q2_0.gguf");
        if !is_unsuffixed_q2_0 {
            return false;
        }
        let has_g64_sibling = self.siblings.iter().any(|s| {
            let sib = s.filename.to_ascii_lowercase();
            sib.ends_with(".gguf") && sib.contains("g64")
        });
        let repo_lower = self.resolve_repo_id().to_ascii_lowercase();
        let is_prism_ternary = repo_lower.starts_with("prism-ml/")
            || repo_lower.contains("ternary-bonsai")
            || self
                .tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case("prismml") || t.eq_ignore_ascii_case("bonsai"));
        has_g64_sibling || is_prism_ternary
    }

    fn is_compatible_gguf_sibling(&self, filename: &str) -> bool {
        if !Self::is_standalone_gguf(filename) {
            return false;
        }
        if !self.is_embedding_model() {
            if let Some(arch) = self.gguf_architecture() {
                if !lattice_core::domain::models::llm_architecture::is_supported_llm_architecture(
                    arch,
                ) {
                    return false;
                }
            }
        }
        if self.requires_fork_hadamard_runtime() || self.is_fork_g128_q2_0(filename) {
            return false;
        }
        true
    }

    fn gguf_variants(&self) -> Vec<ExternalModelMetadata> {
        let base = self.to_external_metadata();
        let mut variants: Vec<_> = self
            .siblings
            .iter()
            .filter(|file| self.is_compatible_gguf_sibling(&file.filename))
            .map(|file| ExternalModelMetadata {
                preferred_filename: Some(file.filename.clone()),
                preferred_size_bytes: file.size,
                ..base.clone()
            })
            .collect();
        variants.sort_by(|a, b| a.preferred_filename.cmp(&b.preferred_filename));
        variants.dedup_by(|a, b| a.preferred_filename == b.preferred_filename);
        variants
    }

    fn resolve_repo_id(&self) -> String {
        self.model_id
            .clone()
            .or_else(|| self.id.clone())
            .unwrap_or_else(|| "unknown".into())
    }

    fn config_model_type(&self) -> Option<&str> {
        self.config
            .as_ref()?
            .get("model_type")?
            .as_str()
            .filter(|model_type| !model_type.trim().is_empty())
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
        } else if lower.contains("f16")
            || lower.contains("bf16")
            || lower.contains("fp16")
            || lower.contains("f32")
            || lower.contains("fp32")
        {
            6
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
            .filter(|s| self.is_compatible_gguf_sibling(&s.filename))
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

    /// Whether the bundled llama.cpp can run this embedding repository: it
    /// needs a single-file GGUF, of an architecture the pinned engine lists.
    /// When Hugging Face has not indexed the GGUF header, the download's own
    /// header check decides.
    fn embedding_compatibility(&self, has_gguf: bool) -> EmbeddingCompatibility {
        let architecture = self
            .gguf_architecture()
            .or_else(|| self.config_model_type())
            .unwrap_or("unknown")
            .to_ascii_lowercase();
        if !has_gguf {
            return EmbeddingCompatibility::Incompatible {
                architecture,
                reason: "The repository ships no single-file GGUF. Lattice runs embedding models from GGUF files in its bundled llama.cpp; look for a GGUF conversion of this model.".to_string(),
            };
        }
        match self.gguf_architecture() {
            Some(arch)
                if !lattice_core::domain::models::llm_architecture::is_supported_llm_architecture(
                    arch,
                ) =>
            {
                EmbeddingCompatibility::Incompatible {
                    architecture,
                    reason: "Lattice's bundled llama.cpp does not support this architecture."
                        .to_string(),
                }
            }
            _ => EmbeddingCompatibility::Compatible { architecture },
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
        let preferred_file = self.select_preferred_gguf_file();

        let mut tags = self.tags.clone();
        if let Some(pipeline) = &self.pipeline_tag {
            if !tags.contains(pipeline) {
                tags.insert(0, pipeline.clone());
            }
        }
        if let Some(model_type) = self.config_model_type() {
            if !tags.iter().any(|tag| tag.eq_ignore_ascii_case(model_type)) {
                tags.push(model_type.to_string());
            }
        }
        let preferred_is_gguf = preferred_file
            .as_ref()
            .is_some_and(|(f, _)| f.to_lowercase().ends_with(".gguf"));
        if preferred_is_gguf && !tags.iter().any(|t| t.eq_ignore_ascii_case("gguf")) {
            tags.push("gguf".into());
        }
        let description = self.build_description(&id, preferred_is_gguf);

        let embedding_compatibility = self
            .is_embedding_model()
            .then(|| self.embedding_compatibility(preferred_is_gguf));

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

#[cfg(test)]
mod tests;
