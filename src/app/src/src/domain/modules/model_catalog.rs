//! # Model Catalog Domain
//!
//! Domain logic for searching and ranking AI models from external catalogs.
//!
//! ## Architecture
//!
//! Following Domain-Driven Design (DDD) patterns:
//! - Value Objects: SearchFilters, ModelSearchResult
//! - Domain Services: ModelCatalogService
//!
//! ## Key Concepts
//!
//! - **SearchFilters**: Filter criteria for model search (category, size, capabilities)
//! - **ModelSearchResult**: Aggregate containing model + relevance score
//! - **ModelCatalogService**: Domain service for search and ranking logic
//!
//! ## Example
//!
//! ```rust
//! use vault_desktop::domain::model_catalog::{SearchFilters, ModelCatalogService};
//! use vault_desktop::domain::model_management::{ModelMetadata, ModelCategory};
//!
//! let filters = SearchFilters {
//!     category: Some(ModelCategory::LLM),
//!     max_size_gb: Some(5.0),
//!     required_capabilities: vec!["chat".into()],
//!     query_text: Some("small chat model".into()),
//! };
//!
//! let service = ModelCatalogService::new();
//! let models = vec![/* external models */];
//! let results = service.search_and_rank(models, &filters);
//! ```

use crate::features::model_management::domain::{ModelCategory, ModelMetadata};
use serde::{Deserialize, Serialize};

// ============================================================================
// Value Objects - Search Filters
// ============================================================================

/// Search filters for model catalog queries.
///
/// This value object encapsulates search criteria for finding models:
/// - Category filtering (LLM, Embedding, OCR)
/// - Size constraints (max_size_gb)
/// - Required capabilities (chat, code, etc.)
/// - Text query matching (name, description)
///
/// # Invariants
/// - max_size_gb must be positive if specified
/// - required_capabilities are case-insensitive
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilters {
    /// Filter by model category (LLM, Embedding, OCR)
    pub category: Option<ModelCategory>,
    /// Maximum model size in gigabytes
    pub max_size_gb: Option<f64>,
    /// Required capabilities (e.g., "chat", "code")
    pub required_capabilities: Vec<String>,
    /// Text query to match against name/description
    pub query_text: Option<String>,
}

impl SearchFilters {
    /// Create a new SearchFilters with validation.
    ///
    /// # Arguments
    /// - `category` - Optional category filter
    /// - `max_size_gb` - Optional size limit (must be positive)
    /// - `required_capabilities` - Required capabilities (normalized to lowercase)
    /// - `query_text` - Optional text query
    ///
    /// # Returns
    /// Validated SearchFilters or error message.
    pub fn new(
        category: Option<ModelCategory>,
        max_size_gb: Option<f64>,
        required_capabilities: Vec<String>,
        query_text: Option<String>,
    ) -> Result<Self, String> {
        // Validate max_size_gb
        if let Some(size) = max_size_gb {
            if size <= 0.0 {
                return Err("max_size_gb must be positive".into());
            }
        }

        // Normalize capabilities to lowercase
        let normalized_capabilities: Vec<String> = required_capabilities
            .into_iter()
            .map(|c| c.to_lowercase())
            .collect();

        Ok(Self {
            category,
            max_size_gb,
            required_capabilities: normalized_capabilities,
            query_text,
        })
    }

    /// Check if a model matches these filters.
    ///
    /// # Arguments
    /// - `model` - Model to check
    ///
    /// # Returns
    /// true if model passes all filter criteria.
    pub fn matches(&self, model: &ModelMetadata) -> bool {
        // Category filter
        if let Some(category) = &self.category {
            if model.category != *category {
                return false;
            }
        }

        // Size filter
        if let Some(max_size) = self.max_size_gb {
            if model.size_gb > max_size {
                return false;
            }
        }

        // Capabilities filter
        for required_cap in &self.required_capabilities {
            let has_capability = model
                .capabilities
                .iter()
                .any(|c| c.to_lowercase() == *required_cap);
            if !has_capability {
                return false;
            }
        }

        // Text query filter (match name or description)
        if let Some(query) = &self.query_text {
            let query_lower = query.to_lowercase();
            let name_match = model.name.to_lowercase().contains(&query_lower);
            let desc_match = model.description.to_lowercase().contains(&query_lower);
            if !name_match && !desc_match {
                return false;
            }
        }

        true
    }
}

// ============================================================================
// Aggregates - Model Search Result
// ============================================================================

/// Model search result with relevance score.
///
/// This aggregate combines:
/// - Model metadata
/// - Relevance score (0-100) based on query match
/// - Source indicator (curated vs external)
///
/// Used for presenting ranked search results to users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSearchResult {
    /// Model metadata
    pub model: ModelMetadata,
    /// Relevance score (0-100)
    pub relevance_score: f64,
    /// Source of the model
    pub source: ModelSource,
}

/// Model source indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelSource {
    /// Curated model from internal catalog
    Curated,
    /// External model from API (Hugging Face)
    External,
}

impl ModelSearchResult {
    /// Create a new search result.
    ///
    /// # Arguments
    /// - `model` - Model metadata
    /// - `relevance_score` - Score 0-100
    /// - `source` - Model source
    pub fn new(model: ModelMetadata, relevance_score: f64, source: ModelSource) -> Self {
        Self {
            model,
            relevance_score: relevance_score.clamp(0.0_f64, 100.0_f64),
            source,
        }
    }

    /// Sort results by relevance (descending).
    pub fn sort_by_relevance(results: &mut [ModelSearchResult]) {
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

// ============================================================================
// Domain Service - Model Catalog Service
// ============================================================================

/// Domain service for searching and ranking models.
///
/// This service implements the search and ranking algorithm as pure business
/// logic with no infrastructure dependencies.
///
/// # Algorithm
///
/// 1. **Filtering**: Apply search filters to remove non-matching models
/// 2. **Relevance Scoring**: Calculate 0-100 score based on:
///    - Text match quality (name/description)
///    - Category match bonus
///    - Capability match bonus
/// 3. **Ranking**: Sort by relevance score (descending)
/// 4. **Source Preference**: Curated models get +5 bonus
///
/// # Example
///
/// ```rust
/// use vault_desktop::domain::model_catalog::{SearchFilters, ModelCatalogService, ModelSource};
/// use vault_desktop::domain::model_management::{ModelMetadata, ModelCategory, PerformanceTier};
///
/// let service = ModelCatalogService::new();
///
/// let models = vec![
///     ModelMetadata {
///         id: "phi-3-mini".into(),
///         name: "Phi-3 Mini".into(),
///         category: ModelCategory::LLM,
///         description: "Compact chat model".into(),
///         size_gb: 2.3,
///         minimum_ram_gb: 4.0,
///         recommended_ram_gb: 8.0,
///         context_length: 4096,
///         performance_tier: PerformanceTier::Fast,
///         supported_quantizations: vec!["Q4_K_M".into()],
///         capabilities: vec!["chat".into(), "code".into()],
///         download_url: None,
///         license: "MIT".into(),
///         requires_auth: false,
///         model_id: None,
///         default_filename: None,
///         files: vec![],
///         total_size_bytes: 0,
///         embedding_dimensions: None,
///     },
/// ];
///
/// let filters = SearchFilters {
///     category: Some(ModelCategory::LLM),
///     max_size_gb: Some(5.0),
///     required_capabilities: vec!["chat".into()],
///     query_text: Some("phi".into()),
/// };
///
/// let results = service.search_and_rank(models, &filters);
/// assert_eq!(results.len(), 1);
/// assert!(results[0].relevance_score > 80.0);
/// ```
pub struct ModelCatalogService;

impl ModelCatalogService {
    /// Create a new model catalog service.
    pub fn new() -> Self {
        Self
    }

    /// Search and rank models based on filters.
    ///
    /// # Arguments
    /// - `models` - Models to search (curated + external)
    /// - `filters` - Search filters
    ///
    /// # Returns
    /// Ranked list of matching models with relevance scores.
    pub fn search_and_rank(
        &self,
        models: Vec<(ModelMetadata, ModelSource)>,
        filters: &SearchFilters,
    ) -> Vec<ModelSearchResult> {
        let mut results: Vec<ModelSearchResult> = models
            .into_iter()
            .filter(|(model, _)| filters.matches(model))
            .map(|(model, source)| {
                let relevance_score = self.calculate_relevance(&model, filters, source);
                ModelSearchResult::new(model, relevance_score, source)
            })
            .collect();

        // Sort by relevance
        ModelSearchResult::sort_by_relevance(&mut results);

        results
    }

    /// Calculate relevance score for a model.
    ///
    /// # Scoring Algorithm
    ///
    /// 1. Base score: 50
    /// 2. Text match bonus: +30 if exact name match, +20 if partial, +10 if description
    /// 3. Category match bonus: +10
    /// 4. Capability match bonus: +5 per matching capability
    /// 5. Source bonus: +5 for curated models
    /// 6. Size bonus: +5 if model is small (<2GB)
    ///
    /// Total: 0-100 (clamped)
    fn calculate_relevance(
        &self,
        model: &ModelMetadata,
        filters: &SearchFilters,
        source: ModelSource,
    ) -> f64 {
        let mut score: f64 = 50.0;

        // Text match bonus
        if let Some(query) = &filters.query_text {
            let query_lower = query.to_lowercase();
            let name_lower = model.name.to_lowercase();
            let desc_lower = model.description.to_lowercase();

            if name_lower == query_lower {
                score += 30.0; // Exact name match
            } else if name_lower.contains(&query_lower) {
                score += 20.0; // Partial name match
            } else if desc_lower.contains(&query_lower) {
                score += 10.0; // Description match
            }
        }

        // Category match bonus
        if let Some(category) = &filters.category {
            if model.category == *category {
                score += 10.0;
            }
        }

        // Capability match bonus
        for required_cap in &filters.required_capabilities {
            let has_capability = model
                .capabilities
                .iter()
                .any(|c| c.to_lowercase() == *required_cap);
            if has_capability {
                score += 5.0;
            }
        }

        // Source bonus (prefer curated)
        if matches!(source, ModelSource::Curated) {
            score += 5.0;
        }

        // Size bonus (prefer smaller models)
        if model.size_gb <= 2.0 {
            score += 5.0;
        }

        score.clamp(0.0_f64, 100.0_f64)
    }
}

impl Default for ModelCatalogService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::model_management::domain::PerformanceTier;

    fn create_test_model(
        id: &str,
        name: &str,
        category: ModelCategory,
        size_gb: f64,
        capabilities: Vec<&str>,
    ) -> ModelMetadata {
        ModelMetadata {
            id: id.into(),
            name: name.into(),
            category,
            description: format!("Description for {}", name),
            size_gb,
            minimum_ram_gb: 4.0,
            recommended_ram_gb: 8.0,
            context_length: 4096,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["Q4_K_M".into()],
            capabilities: capabilities.into_iter().map(|s| s.into()).collect(),
            download_url: None,
            license: "MIT".into(),
            requires_auth: false,
            model_id: None,
            default_filename: None,
            files: vec![],
            total_size_bytes: 0,
            embedding_dimensions: None,
        }
    }

    #[test]
    fn test_search_filters_new_valid() {
        let filters = SearchFilters::new(
            Some(ModelCategory::LLM),
            Some(5.0),
            vec!["chat".into(), "Code".into()],
            Some("test".into()),
        )
        .unwrap();

        assert_eq!(filters.category, Some(ModelCategory::LLM));
        assert_eq!(filters.max_size_gb, Some(5.0));
        assert_eq!(filters.required_capabilities, vec!["chat", "code"]);
        assert_eq!(filters.query_text, Some("test".into()));
    }

    #[test]
    fn test_search_filters_new_invalid_size() {
        let result = SearchFilters::new(None, Some(-1.0), vec![], None);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("positive"));
    }

    #[test]
    fn test_search_filters_matches_category() {
        let filters = SearchFilters {
            category: Some(ModelCategory::LLM),
            max_size_gb: None,
            required_capabilities: vec![],
            query_text: None,
        };

        let llm_model =
            create_test_model("llm", "LLM Model", ModelCategory::LLM, 2.0, vec!["chat"]);
        let embedding_model = create_test_model(
            "embed",
            "Embedding",
            ModelCategory::Embedding,
            1.0,
            vec!["embedding"],
        );

        assert!(filters.matches(&llm_model));
        assert!(!filters.matches(&embedding_model));
    }

    #[test]
    fn test_search_filters_matches_size() {
        let filters = SearchFilters {
            category: None,
            max_size_gb: Some(2.0),
            required_capabilities: vec![],
            query_text: None,
        };

        let small_model = create_test_model(
            "small",
            "Small Model",
            ModelCategory::LLM,
            1.5,
            vec!["chat"],
        );
        let large_model = create_test_model(
            "large",
            "Large Model",
            ModelCategory::LLM,
            5.0,
            vec!["chat"],
        );

        assert!(filters.matches(&small_model));
        assert!(!filters.matches(&large_model));
    }

    #[test]
    fn test_search_filters_matches_capabilities() {
        let filters = SearchFilters {
            category: None,
            max_size_gb: None,
            required_capabilities: vec!["chat".into(), "code".into()],
            query_text: None,
        };

        let chat_code_model = create_test_model(
            "both",
            "Both",
            ModelCategory::LLM,
            2.0,
            vec!["chat", "code"],
        );
        let chat_only_model =
            create_test_model("chat", "Chat Only", ModelCategory::LLM, 2.0, vec!["chat"]);

        assert!(filters.matches(&chat_code_model));
        assert!(!filters.matches(&chat_only_model));
    }

    #[test]
    fn test_search_filters_matches_text_query() {
        let filters = SearchFilters {
            category: None,
            max_size_gb: None,
            required_capabilities: vec![],
            query_text: Some("phi".into()),
        };

        let phi_model =
            create_test_model("phi-3", "Phi-3 Mini", ModelCategory::LLM, 2.0, vec!["chat"]);
        let other_model =
            create_test_model("llama", "Llama 3", ModelCategory::LLM, 4.0, vec!["chat"]);

        assert!(filters.matches(&phi_model));
        assert!(!filters.matches(&other_model));
    }

    #[test]
    fn test_model_search_result_creation() {
        let model = create_test_model("test", "Test Model", ModelCategory::LLM, 2.0, vec!["chat"]);
        let result = ModelSearchResult::new(model.clone(), 85.5, ModelSource::Curated);

        assert_eq!(result.model.id, "test");
        assert_eq!(result.relevance_score, 85.5);
        assert_eq!(result.source, ModelSource::Curated);
    }

    #[test]
    fn test_model_search_result_score_clamping() {
        let model = create_test_model("test", "Test Model", ModelCategory::LLM, 2.0, vec!["chat"]);

        let result1 = ModelSearchResult::new(model.clone(), 150.0, ModelSource::Curated);
        assert_eq!(result1.relevance_score, 100.0);

        let result2 = ModelSearchResult::new(model.clone(), -10.0, ModelSource::External);
        assert_eq!(result2.relevance_score, 0.0);
    }

    #[test]
    fn test_model_search_result_sorting() {
        let model1 = create_test_model("m1", "Model 1", ModelCategory::LLM, 2.0, vec!["chat"]);
        let model2 = create_test_model("m2", "Model 2", ModelCategory::LLM, 3.0, vec!["chat"]);
        let model3 = create_test_model("m3", "Model 3", ModelCategory::LLM, 1.0, vec!["chat"]);

        let mut results = vec![
            ModelSearchResult::new(model1, 50.0, ModelSource::Curated),
            ModelSearchResult::new(model2, 90.0, ModelSource::External),
            ModelSearchResult::new(model3, 70.0, ModelSource::Curated),
        ];

        ModelSearchResult::sort_by_relevance(&mut results);

        assert_eq!(results[0].relevance_score, 90.0);
        assert_eq!(results[1].relevance_score, 70.0);
        assert_eq!(results[2].relevance_score, 50.0);
    }

    #[test]
    fn test_catalog_service_search_and_rank() {
        let service = ModelCatalogService::new();

        let models = vec![
            (
                create_test_model(
                    "phi-3",
                    "Phi-3 Mini",
                    ModelCategory::LLM,
                    2.3,
                    vec!["chat", "code"],
                ),
                ModelSource::Curated,
            ),
            (
                create_test_model("llama", "Llama 3", ModelCategory::LLM, 4.7, vec!["chat"]),
                ModelSource::External,
            ),
            (
                create_test_model(
                    "embed",
                    "Embedding",
                    ModelCategory::Embedding,
                    1.0,
                    vec!["embedding"],
                ),
                ModelSource::Curated,
            ),
        ];

        let filters = SearchFilters {
            category: Some(ModelCategory::LLM),
            max_size_gb: Some(5.0),
            required_capabilities: vec!["chat".into()],
            query_text: None,
        };

        let results = service.search_and_rank(models, &filters);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].model.id, "phi-3"); // Higher score (curated, small)
        assert_eq!(results[1].model.id, "llama");
    }

    #[test]
    fn test_catalog_service_calculate_relevance_exact_name_match() {
        let service = ModelCatalogService::new();
        let model = create_test_model("phi-3", "Phi-3 Mini", ModelCategory::LLM, 2.0, vec!["chat"]);

        let filters = SearchFilters {
            category: Some(ModelCategory::LLM),
            max_size_gb: None,
            required_capabilities: vec![],
            query_text: Some("phi-3 mini".into()),
        };

        let score = service.calculate_relevance(&model, &filters, ModelSource::Curated);

        // Base(50) + ExactName(30) + Category(10) + Curated(5) + Small(5) = 100
        assert_eq!(score, 100.0);
    }

    #[test]
    fn test_catalog_service_calculate_relevance_partial_match() {
        let service = ModelCatalogService::new();
        let model = create_test_model("phi-3", "Phi-3 Mini", ModelCategory::LLM, 2.0, vec!["chat"]);

        let filters = SearchFilters {
            category: Some(ModelCategory::LLM),
            max_size_gb: None,
            required_capabilities: vec!["chat".into()],
            query_text: Some("phi".into()),
        };

        let score = service.calculate_relevance(&model, &filters, ModelSource::Curated);

        // Base(50) + PartialName(20) + Category(10) + Capability(5) + Curated(5) + Small(5) = 95
        assert_eq!(score, 95.0);
    }

    #[test]
    fn test_catalog_service_empty_results() {
        let service = ModelCatalogService::new();
        let models = vec![(
            create_test_model(
                "large",
                "Large Model",
                ModelCategory::LLM,
                10.0,
                vec!["chat"],
            ),
            ModelSource::External,
        )];

        let filters = SearchFilters {
            category: None,
            max_size_gb: Some(5.0),
            required_capabilities: vec![],
            query_text: None,
        };

        let results = service.search_and_rank(models, &filters);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_catalog_service_curated_vs_external_ranking() {
        let service = ModelCatalogService::new();

        let curated =
            create_test_model("c1", "Curated Model", ModelCategory::LLM, 3.0, vec!["chat"]);
        let external = create_test_model(
            "e1",
            "External Model",
            ModelCategory::LLM,
            3.0,
            vec!["chat"],
        );

        let models = vec![
            (external, ModelSource::External),
            (curated, ModelSource::Curated),
        ];

        let filters = SearchFilters::default();
        let results = service.search_and_rank(models, &filters);

        // Curated should rank higher due to +5 bonus
        assert_eq!(results[0].source, ModelSource::Curated);
        assert_eq!(results[1].source, ModelSource::External);
    }
}
