//! Get Best Model Use Case
//!
//! Selects the single best model for the current system.
//!
//! # Purpose
//!
//! Simplifies model selection by automatically choosing the best model
//! based on system capabilities. This delegates to GetRecommendedModelsUseCase
//! and returns the top recommendation.
//!
//! # Dependencies
//! - `GetRecommendedModelsUseCase` - Get recommendations
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetBestModelUseCase::new(get_recommended_models_use_case);
//! let best = use_case.execute().await?;
//! println!("Best model: {} - {}", best.model.name, best.reason);
//! ```

use crate::features::llm::dto::{BestModelDto, GetRecommendationsRequestDto};
use crate::features::llm::use_cases::GetRecommendedModelsUseCase;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetBestModelUseCase {
    get_recommendations: Arc<GetRecommendedModelsUseCase>,
}

impl GetBestModelUseCase {
    pub fn new(get_recommendations: Arc<GetRecommendedModelsUseCase>) -> Self {
        Self {
            get_recommendations,
        }
    }

    pub async fn execute(&self) -> Result<BestModelDto, AppError> {
        // Get recommendations (auto-detect tier)
        let request = GetRecommendationsRequestDto { tier: None };
        let recommendations = self.get_recommendations.execute(request).await?;

        // Return first recommendation (highest ranked)
        recommendations
            .recommendations
            .into_iter()
            .next()
            .map(|rec| BestModelDto {
                model: rec.model,
                reason: rec.reason,
            })
            .ok_or_else(|| {
                AppError::InvalidState(
                    "No suitable models found for your system. \
                     Your system may have insufficient RAM to run any available models. \
                     Please upgrade your system or close other applications to free up memory."
                        .to_string(),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::model_catalog::MockModelCatalogPort;
    use crate::application::ports::system_info::MockSystemInfoPort;
    use crate::features::llm::use_cases::GetSystemCapabilitiesUseCase;

    fn create_use_case(
        system_info: MockSystemInfoPort,
        catalog: MockModelCatalogPort,
    ) -> GetBestModelUseCase {
        let system_capabilities =
            Arc::new(GetSystemCapabilitiesUseCase::new(Arc::new(system_info)));
        let get_recommendations = Arc::new(GetRecommendedModelsUseCase::new(
            system_capabilities,
            Arc::new(catalog),
        ));
        GetBestModelUseCase::new(get_recommendations)
    }

    #[tokio::test]
    async fn test_returns_best_model() {
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let best = result.unwrap();
        assert!(!best.model.id.is_empty());
        assert!(!best.model.name.is_empty());
        assert!(!best.reason.is_empty());
        assert!(best.model.size_gb > 0.0);
    }

    #[tokio::test]
    async fn test_low_end_system_gets_small_model() {
        let system_info = MockSystemInfoPort::low_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let result = use_case.execute().await.unwrap();

        // Should recommend a small model for low-end system
        assert!(result.model.minimum_ram_gb <= 4.0);
        assert!(result.model.size_gb < 3.0);
    }

    #[tokio::test]
    async fn test_high_end_system_gets_large_model() {
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let result = use_case.execute().await.unwrap();

        // Should recommend a larger, more capable model
        // High-end system has 16GB available, so can run models up to 16GB min RAM
        assert!(result.model.minimum_ram_gb <= 16.0);
        assert!(result.model.capabilities.len() >= 2); // Should have multiple capabilities
    }

    #[tokio::test]
    async fn test_reason_includes_system_info() {
        let system_info = MockSystemInfoPort::medium_tier();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let result = use_case.execute().await.unwrap();

        // Reason should mention tier
        let reason_lower = result.reason.to_lowercase();
        assert!(
            reason_lower.contains("medium") || reason_lower.contains("tier"),
            "Reason should mention system tier: {}",
            result.reason
        );
    }

    #[tokio::test]

    async fn test_no_suitable_models_returns_error() {
        // Very low RAM system
        let system_info = MockSystemInfoPort::low_end();
        system_info.set_ram(0.5); // 0.5GB total
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let result = use_case.execute().await;

        assert!(result.is_err());
        match result {
            Err(AppError::InvalidState(msg)) => {
                assert!(msg.contains("No suitable models"));
                assert!(msg.contains("insufficient RAM") || msg.contains("free up memory"));
            }
            _ => panic!("Expected InvalidState error"),
        }
    }

    #[tokio::test]
    async fn test_empty_catalog_returns_error() {
        let system_info = MockSystemInfoPort::high_end();
        let catalog = MockModelCatalogPort::empty();
        let use_case = create_use_case(system_info, catalog);

        let result = use_case.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_model_has_valid_metadata() {
        let system_info = MockSystemInfoPort::new();
        let catalog = MockModelCatalogPort::new();
        let use_case = create_use_case(system_info, catalog);

        let result = use_case.execute().await.unwrap();

        // Verify model has complete metadata
        assert!(!result.model.id.is_empty());
        assert!(!result.model.name.is_empty());
        assert!(result.model.size_gb > 0.0);
        assert!(result.model.context_length > 0);
        assert!(!result.model.capabilities.is_empty());
        assert!(result.model.minimum_ram_gb > 0.0);
    }

    #[tokio::test]
    async fn test_different_systems_get_different_models() {
        // Low-end system
        let low_system = MockSystemInfoPort::low_end();
        let low_catalog = MockModelCatalogPort::new();
        let low_use_case = create_use_case(low_system, low_catalog);
        let low_result = low_use_case.execute().await.unwrap();

        // High-end system
        let high_system = MockSystemInfoPort::high_end();
        let high_catalog = MockModelCatalogPort::new();
        let high_use_case = create_use_case(high_system, high_catalog);
        let high_result = high_use_case.execute().await.unwrap();

        // Different systems should potentially get different models
        // (or at minimum, high-end should be able to run larger models)
        assert!(high_result.model.minimum_ram_gb >= low_result.model.minimum_ram_gb);
    }
}
