//! Model catalog adapter implementation.
//!
//! Deprecated compatibility adapter for the current model-management system.
//! Use `HuggingFaceAdapter` + `ModelCacheAdapter` for external model catalogs.
//! This adapter remains for backward compatibility with existing use cases.
//!
//! Production adapter with hardcoded LLM model catalog.

use crate::application::ports::model_catalog::{ExternalModelMetadata, ModelCatalogPort};
use crate::shared::error::AppError;
use async_trait::async_trait;

pub use self::model_info::ModelInfo;

mod model_info {
    use super::*;

    /// DEPRECATED: Old model info structure (replaced by ExternalModelMetadata)
    /// Re-exported for backward compatibility with existing use cases
    #[derive(Debug, Clone)]
    pub struct ModelInfo {
        pub id: String,
        pub name: String,
        pub size_gb: f64,
        pub context_length: u32,
        pub capabilities: Vec<String>,
        pub minimum_ram_gb: f64,
    }

    impl ModelInfo {
        /// Convert old ModelInfo to new ExternalModelMetadata format
        pub(super) fn to_external_metadata(&self) -> ExternalModelMetadata {
            ExternalModelMetadata {
                id: self.id.clone(),
                name: self.name.clone(),
                description: format!(
                    "Context: {} tokens. Capabilities: {}. Min RAM: {}GB",
                    self.context_length,
                    self.capabilities.join(", "),
                    self.minimum_ram_gb
                ),
                tags: self.capabilities.clone(),
                downloads: 0,
                likes: 0,
                download_url: None,
                license: "Unknown".to_string(),
                last_modified: "Unknown".to_string(),
                gated: None,
                preferred_filename: None,
                preferred_size_bytes: None,
                embedding_compatibility: None,
            }
        }
    }
}

/// Hardcoded model catalog.
///
/// Provides a static catalog of available LLM models.
/// In the future, this could be loaded from a JSON file or remote API.
pub struct HardcodedModelCatalog {
    models: Vec<model_info::ModelInfo>,
}

impl HardcodedModelCatalog {
    /// Create a new hardcoded model catalog.
    pub fn new() -> Self {
        Self {
            models: vec![
                // Small models (<2GB) - Good for low-end hardware
                model_info::ModelInfo {
                    id: "tinyllama".to_string(),
                    name: "TinyLlama 1.1B".to_string(),
                    size_gb: 1.1,
                    context_length: 2048,
                    capabilities: vec!["chat".to_string()],
                    minimum_ram_gb: 2.0,
                },
                model_info::ModelInfo {
                    id: "phi-3-mini".to_string(),
                    name: "Phi-3 Mini".to_string(),
                    size_gb: 1.8,
                    context_length: 4096,
                    capabilities: vec!["chat".to_string(), "code".to_string()],
                    minimum_ram_gb: 4.0,
                },
                // Medium models (2-8GB) - Good for most users
                ModelInfo {
                    id: "mistral-7b".to_string(),
                    name: "Mistral 7B".to_string(),
                    size_gb: 4.1,
                    context_length: 8192,
                    capabilities: vec![
                        "chat".to_string(),
                        "code".to_string(),
                        "reasoning".to_string(),
                    ],
                    minimum_ram_gb: 8.0,
                },
                ModelInfo {
                    id: "llama-3.2-7b".to_string(),
                    name: "Llama 3.2 7B".to_string(),
                    size_gb: 4.7,
                    context_length: 8192,
                    capabilities: vec!["chat".to_string(), "code".to_string()],
                    minimum_ram_gb: 10.0,
                },
                ModelInfo {
                    id: "gemma-7b".to_string(),
                    name: "Gemma 7B".to_string(),
                    size_gb: 5.0,
                    context_length: 8192,
                    capabilities: vec!["chat".to_string(), "instruct".to_string()],
                    minimum_ram_gb: 10.0,
                },
                // Large models (>8GB) - Require high-end hardware
                ModelInfo {
                    id: "llama-3.1-13b".to_string(),
                    name: "Llama 3.1 13B".to_string(),
                    size_gb: 7.3,
                    context_length: 16384,
                    capabilities: vec![
                        "chat".to_string(),
                        "code".to_string(),
                        "reasoning".to_string(),
                    ],
                    minimum_ram_gb: 16.0,
                },
                ModelInfo {
                    id: "mixtral-8x7b".to_string(),
                    name: "Mixtral 8x7B".to_string(),
                    size_gb: 26.0,
                    context_length: 32768,
                    capabilities: vec![
                        "chat".to_string(),
                        "code".to_string(),
                        "reasoning".to_string(),
                    ],
                    minimum_ram_gb: 32.0,
                },
                ModelInfo {
                    id: "llama-3.1-70b".to_string(),
                    name: "Llama 3.1 70B".to_string(),
                    size_gb: 40.0,
                    context_length: 131072,
                    capabilities: vec![
                        "chat".to_string(),
                        "code".to_string(),
                        "reasoning".to_string(),
                        "long-context".to_string(),
                    ],
                    minimum_ram_gb: 64.0,
                },
            ],
        }
    }
}

impl Default for HardcodedModelCatalog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelCatalogPort for HardcodedModelCatalog {
    async fn search_models(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ExternalModelMetadata>, AppError> {
        let filtered: Vec<ExternalModelMetadata> = if query.is_empty() {
            self.models
                .iter()
                .map(|m| m.to_external_metadata())
                .collect()
        } else {
            // Simple substring matching on id, name, and capabilities
            let query_lower = query.to_lowercase();
            self.models
                .iter()
                .filter(|m| {
                    m.id.to_lowercase().contains(&query_lower)
                        || m.name.to_lowercase().contains(&query_lower)
                        || m.capabilities
                            .iter()
                            .any(|c| c.to_lowercase().contains(&query_lower))
                })
                .map(|m| m.to_external_metadata())
                .collect()
        };

        let limited: Vec<ExternalModelMetadata> = filtered.into_iter().take(limit).collect();

        Ok(limited)
    }

    async fn get_model_by_id(
        &self,
        model_id: &str,
    ) -> Result<Option<ExternalModelMetadata>, AppError> {
        Ok(self
            .models
            .iter()
            .find(|m| m.id == model_id)
            .map(|m| m.to_external_metadata()))
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     #[tokio::test]
//     async fn test_catalog_has_models() {
//         let catalog = HardcodedModelCatalog::new();
//         let models = catalog.get_all_models().await.unwrap();
//         assert!(models.len() >= 7);
//     }
//     #[tokio::test]
//     async fn test_catalog_get_by_id() {
//         let catalog = HardcodedModelCatalog::new();
//         let model = catalog.get_model_by_id("phi-3-mini").await.unwrap();
//         assert!(model.is_some());
//         let model = model.unwrap();
//         assert_eq!(model.id, "phi-3-mini");
//         assert_eq!(model.name, "Phi-3 Mini");
//     }
//     #[tokio::test]
//     async fn test_catalog_get_by_id_not_found() {
//         let catalog = HardcodedModelCatalog::new();
//         let model = catalog.get_model_by_id("nonexistent").await.unwrap();
//         assert!(model.is_none());
//     }
//     #[tokio::test]
//     async fn test_catalog_all_models_have_required_fields() {
//         let catalog = HardcodedModelCatalog::new();
//         let models = catalog.get_all_models().await.unwrap();
//         for model in models {
//             assert!(!model.id.is_empty());
//             assert!(!model.name.is_empty());
//             assert!(model.size_gb > 0.0);
//             assert!(model.context_length > 0);
//             assert!(!model.capabilities.is_empty());
//             assert!(model.minimum_ram_gb > 0.0);
//         }
//     }
// }
