//! Get Recommended Models Use Case
//!
//! Recommends LLM models based on system capabilities.
//!
//! # Purpose
//!
//! Analyzes system hardware (RAM, GPU) and recommends the top 3 models
//! that will run well on the current system. Uses performance tiers
//! (Low/Medium/High) based on available RAM.
//!
//! # Dependencies
//! - `GetSystemCapabilitiesUseCase` - Detect hardware
//! - `ModelCatalogPort` - Access model catalog
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetRecommendedModelsUseCase::new(
//!     system_capabilities_use_case,
//!     model_catalog,
//! );
//! let request = GetRecommendationsRequestDto { tier: None }; // Auto-detect
//! let recommendations = use_case.execute(request).await?;
//! ```

use crate::features::llm::dto::{
    GetRecommendationsRequestDto, ModelInfoDto, PerformanceTier, RecommendedModelDto,
    RecommendedModelsDto,
};
use crate::application::ports::model_catalog::{ExternalModelMetadata, ModelCatalogPort};
use crate::features::llm::use_cases::GetSystemCapabilitiesUseCase;
use crate::features::model_management::domain::ModelMetadata;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetRecommendedModelsUseCase {
    system_capabilities: Arc<GetSystemCapabilitiesUseCase>,
    model_catalog: Arc<dyn ModelCatalogPort>,
}

impl GetRecommendedModelsUseCase {
    pub fn new(
        system_capabilities: Arc<GetSystemCapabilitiesUseCase>,
        model_catalog: Arc<dyn ModelCatalogPort>,
    ) -> Self {
        Self {
            system_capabilities,
            model_catalog,
        }
    }

    pub async fn execute(
        &self,
        request: GetRecommendationsRequestDto,
    ) -> Result<RecommendedModelsDto, AppError> {
        // 1. Determine performance tier
        let (tier, total_ram_gb, has_gpu) = match request.tier {
            Some(t) => {
                // Explicit tier - get capabilities for GPU info
                let caps = self.system_capabilities.execute().await?;
                (t, Self::tier_to_ram(&t), caps.gpu_info.is_some())
            }
            None => {
                // Auto-detect from system
                let caps = self.system_capabilities.execute().await?;
                let tier = Self::determine_tier(caps.total_ram_gb);
                (tier, caps.total_ram_gb, caps.gpu_info.is_some())
            }
        };

        // 2. Get all models from catalog (search with empty query to get all)
        let external_models = self.model_catalog.search_models("", 100).await?;

        // 3. Convert external models to domain models and filter by total RAM
        let mut suitable_models: Vec<ModelMetadata> = Vec::new();
        for external in external_models {
            if let Ok(domain) = external.to_domain_model() {
                if domain.minimum_ram_gb <= total_ram_gb {
                    suitable_models.push(domain);
                }
            }
        }

        if suitable_models.is_empty() {
            return Err(AppError::InvalidState(format!(
                "No suitable models found for {:.1} GB total RAM due to insufficient RAM. Please upgrade your system or close other applications to free up memory.",
                total_ram_gb
            )));
        }

        // 4. Rank models (prefer larger models with more capabilities that fit)
        let mut ranked = Self::rank_models(suitable_models, &tier, has_gpu);

        // 5. Take top 3 recommendations
        ranked.truncate(3);

        // 6. Transform to DTOs
        let recommendations = ranked
            .into_iter()
            .map(|(model, reason)| RecommendedModelDto {
                model: Self::model_to_dto(model),
                reason,
                tier,
            })
            .collect();

        Ok(RecommendedModelsDto { recommendations })
    }

    /// Determine performance tier from total RAM.
    fn determine_tier(total_ram_gb: f64) -> PerformanceTier {
        if total_ram_gb < 8.0 {
            PerformanceTier::Low
        } else if total_ram_gb < 16.0 {
            PerformanceTier::Medium
        } else {
            PerformanceTier::High
        }
    }

    /// Convert tier to total RAM for explicit tier requests.
    fn tier_to_ram(tier: &PerformanceTier) -> f64 {
        match tier {
            PerformanceTier::Low => 4.0,
            PerformanceTier::Medium => 12.0,
            PerformanceTier::High => 32.0,
        }
    }

    /// Rank models by capability/size trade-off.
    ///
    /// Ranking strategy:
    /// 1. Prefer models with more capabilities
    /// 2. Among equal capabilities, prefer larger context
    /// 3. Among equal context, prefer larger model size
    fn rank_models(
        mut models: Vec<ModelMetadata>,
        tier: &PerformanceTier,
        has_gpu: bool,
    ) -> Vec<(ModelMetadata, String)> {
        // Sort by: capabilities count (desc), context length (desc), size (desc)
        models.sort_by(|a, b| {
            b.capabilities
                .len()
                .cmp(&a.capabilities.len())
                .then(b.context_length.cmp(&a.context_length))
                .then(
                    b.size_gb
                        .partial_cmp(&a.size_gb)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });

        models
            .into_iter()
            .map(|m| {
                let reason = Self::generate_reason(&m, tier, has_gpu);
                (m, reason)
            })
            .collect()
    }

    /// Generate recommendation reasoning.
    fn generate_reason(model: &ModelMetadata, tier: &PerformanceTier, has_gpu: bool) -> String {
        let tier_name = match tier {
            PerformanceTier::Low => "low-end",
            PerformanceTier::Medium => "medium-tier",
            PerformanceTier::High => "high-end",
        };

        let capabilities = model.capabilities.join(", ");
        let context_info = format!("{} context window", model.context_length);

        let mut reason = format!(
            "Excellent {} model for your system. Supports {} with a {}",
            tier_name, capabilities, context_info
        );

        if has_gpu {
            reason.push_str(". GPU acceleration available for improved performance");
        }

        reason
    }

    /// Convert ModelMetadata to ModelInfoDto.
    fn model_to_dto(model: ModelMetadata) -> ModelInfoDto {
        ModelInfoDto {
            id: model.id,
            name: model.name,
            size_gb: model.size_gb,
            context_length: model.context_length,
            capabilities: model.capabilities,
            minimum_ram_gb: model.minimum_ram_gb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::model_catalog::MockModelCatalogPort;
    use crate::application::ports::system_info::MockSystemInfoPort;

    fn create_use_case(
        system_info: MockSystemInfoPort,
        catalog: MockModelCatalogPort,
    ) -> GetRecommendedModelsUseCase {
        let system_capabilities =
            Arc::new(GetSystemCapabilitiesUseCase::new(Arc::new(system_info)));
        GetRecommendedModelsUseCase::new(system_capabilities, Arc::new(catalog))
    }

    #[tokio::test]

    async fn test_low_ram_system_recommends_small_models_only() {
        // System with 4GB total RAM
        let system_info = MockSystemInfoPort::low_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto { tier: None };
        let result = use_case.execute(request).await.unwrap();

        // Should only recommend models with minimum_ram_gb <= 4.0
        assert!(!result.recommendations.is_empty());
        for rec in &result.recommendations {
            assert!(rec.model.minimum_ram_gb <= 4.0);
            assert_eq!(rec.tier, PerformanceTier::Low);
        }

        // Should contain tinyllama (1.1GB, 2GB min RAM)
        let has_tinyllama = result
            .recommendations
            .iter()
            .any(|r| r.model.id == "tinyllama");
        assert!(has_tinyllama);
    }

    #[tokio::test]

    async fn test_medium_ram_system_recommends_medium_models() {
        // System with 12GB total RAM
        let system_info = MockSystemInfoPort::medium_tier();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto { tier: None };
        let result = use_case.execute(request).await.unwrap();

        // Should recommend models that fit in 12GB total RAM
        assert!(!result.recommendations.is_empty());
        for rec in &result.recommendations {
            assert!(rec.model.minimum_ram_gb <= 12.0);
            assert_eq!(rec.tier, PerformanceTier::Medium);
        }

        // Should include phi-3-mini (4GB min RAM)
        let has_phi3 = result
            .recommendations
            .iter()
            .any(|r| r.model.id == "phi-3-mini");
        assert!(has_phi3);
    }

    #[tokio::test]

    async fn test_high_ram_system_recommends_large_models() {
        // System with 32GB total RAM
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto { tier: None };
        let result = use_case.execute(request).await.unwrap();

        // Should recommend larger models
        assert!(!result.recommendations.is_empty());
        for rec in &result.recommendations {
            assert!(rec.model.minimum_ram_gb <= 32.0);
            assert_eq!(rec.tier, PerformanceTier::High);
        }

        // Should include mixtral-8x7b (largest model in MockModelCatalogPort)
        let has_mixtral = result
            .recommendations
            .iter()
            .any(|r| r.model.id == "mixtral-8x7b");
        assert!(has_mixtral);
    }

    #[tokio::test]
    async fn test_explicit_tier_override() {
        // High-end system but request low-tier models
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto {
            tier: Some(PerformanceTier::Low),
        };
        let result = use_case.execute(request).await.unwrap();

        // Should respect explicit tier
        for rec in &result.recommendations {
            assert_eq!(rec.tier, PerformanceTier::Low);
            assert!(rec.model.minimum_ram_gb <= 4.0);
        }
    }

    #[tokio::test]
    async fn test_returns_max_three_recommendations() {
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto { tier: None };
        let result = use_case.execute(request).await.unwrap();

        // Should return at most 3 recommendations
        assert!(result.recommendations.len() <= 3);
    }

    #[tokio::test]
    async fn test_gpu_mentioned_in_reasoning() {
        // High-end system with GPU
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto { tier: None };
        let result = use_case.execute(request).await.unwrap();

        // At least one recommendation should mention GPU
        let has_gpu_mention = result
            .recommendations
            .iter()
            .any(|r| r.reason.contains("GPU"));
        assert!(has_gpu_mention);
    }

    #[tokio::test]
    async fn test_no_suitable_models_returns_error() {
        // Very low RAM system
        let system_info = MockSystemInfoPort::low_end();
        system_info.set_ram(1.0); // 1GB total
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto { tier: None };
        let result = use_case.execute(request).await;

        // Should return error with helpful message
        assert!(result.is_err());
        match result {
            Err(AppError::InvalidState(msg)) => {
                assert!(msg.contains("No suitable models"));
                assert!(msg.contains("GB total RAM"));
            }
            _ => panic!("Expected InvalidState error"),
        }
    }

    #[tokio::test]
    async fn test_empty_catalog_returns_error() {
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::empty();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto { tier: None };
        let result = use_case.execute(request).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tier_determination() {
        // Test tier boundaries
        assert_eq!(
            GetRecommendedModelsUseCase::determine_tier(4.0),
            PerformanceTier::Low
        );
        assert_eq!(
            GetRecommendedModelsUseCase::determine_tier(7.9),
            PerformanceTier::Low
        );
        assert_eq!(
            GetRecommendedModelsUseCase::determine_tier(8.0),
            PerformanceTier::Medium
        );
        assert_eq!(
            GetRecommendedModelsUseCase::determine_tier(15.9),
            PerformanceTier::Medium
        );
        assert_eq!(
            GetRecommendedModelsUseCase::determine_tier(16.0),
            PerformanceTier::High
        );
        assert_eq!(
            GetRecommendedModelsUseCase::determine_tier(32.0),
            PerformanceTier::High
        );
    }

    #[tokio::test]
    async fn test_ranking_prefers_more_capabilities() {
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto { tier: None };
        let result = use_case.execute(request).await.unwrap();

        // First recommendation should have most capabilities
        if result.recommendations.len() >= 2 {
            let first = &result.recommendations[0];
            let second = &result.recommendations[1];

            // First should have >= capabilities than second
            assert!(first.model.capabilities.len() >= second.model.capabilities.len());
        }
    }

    #[tokio::test]
    async fn test_reasoning_includes_capabilities() {
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let request = GetRecommendationsRequestDto { tier: None };
        let result = use_case.execute(request).await.unwrap();

        // All recommendations should mention capabilities
        for rec in &result.recommendations {
            // Reason should mention at least one capability
            let has_capability_mention = rec
                .model
                .capabilities
                .iter()
                .any(|cap| rec.reason.to_lowercase().contains(&cap.to_lowercase()));
            assert!(
                has_capability_mention || rec.reason.contains("chat"),
                "Reason should mention capabilities: {}",
                rec.reason
            );
        }
    }
}
