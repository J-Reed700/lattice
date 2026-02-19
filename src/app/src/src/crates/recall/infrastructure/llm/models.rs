//! Model information and catalog
//!
//! Provides model metadata, family classifications, and model recommendations.

use serde::{Deserialize, Serialize};

/// Information about an LLM model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub family: ModelFamily,
    pub quantization: Option<Quantization>,
    pub size_mb: u64,
}

/// Model family classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelFamily {
    Llama,
    Mistral,
    Phi,
    Gemma,
    Other,
}

/// Quantization level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quantization {
    Q4,
    Q5,
    Q8,
    F16,
    F32,
}

/// Model catalog
#[derive(Debug, Clone, Default)]
pub struct ModelCatalog {
    models: Vec<ModelInfo>,
}

impl ModelCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn models(&self) -> &[ModelInfo] {
        &self.models
    }

    pub fn get_by_id(&self, id: &str) -> Option<&ModelInfo> {
        self.models.iter().find(|m| m.name == id)
    }

    pub fn all_models(&self) -> Vec<&ModelInfo> {
        self.models.iter().collect()
    }
}

/// Get the model catalog
pub fn get_catalog() -> ModelCatalog {
    ModelCatalog::new()
}

/// Performance tier for model recommendations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerformanceTier {
    Low,
    Medium,
    High,
}

/// Recommended model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedModel {
    pub model: ModelInfo,
    pub tier: PerformanceTier,
}

/// Hardware capabilities for model recommendations
#[derive(Debug, Clone, Default)]
pub struct HardwareCapabilities {
    pub ram_gb: u64,
    pub has_gpu: bool,
}

/// Model recommender
#[derive(Debug, Clone, Default)]
pub struct ModelRecommender;

impl ModelRecommender {
    pub fn new() -> Self {
        Self
    }

    pub fn recommend(&self, _tier: PerformanceTier) -> Option<RecommendedModel> {
        None
    }

    pub fn best_model(_caps: &HardwareCapabilities) -> Option<ModelInfo> {
        None // Placeholder - would analyze capabilities and return best model
    }
}
