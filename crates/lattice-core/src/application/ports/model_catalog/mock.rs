//! In-memory catalog seeded with sample models, for tests and offline runs.
use super::{ExternalModelMetadata, ModelCatalogPort};
use crate::shared::error::AppError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

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
                preferred_filename: Some("tinyllama-q4.gguf".into()),
                preferred_size_bytes: Some(1_100_000_000),
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
                preferred_filename: Some("phi-3-mini-q4.gguf".into()),
                preferred_size_bytes: Some(1_800_000_000),
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
                preferred_filename: Some("llama-3.2-3b-q4.gguf".into()),
                preferred_size_bytes: Some(2_300_000_000),
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
                preferred_filename: Some("llama-3.2-7b-q4.gguf".into()),
                preferred_size_bytes: Some(4_500_000_000),
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
                preferred_filename: Some("qwen2.5-7b-q4.gguf".into()),
                preferred_size_bytes: Some(4_500_000_000),
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
                preferred_filename: Some("mixtral-8x7b-q4.gguf".into()),
                preferred_size_bytes: Some(26_000_000_000),
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

        matching.sort_by_key(|model| std::cmp::Reverse(model.downloads));

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
