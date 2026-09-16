pub mod fusion;

pub use fusion::{FusionStrategy, RrfFusion, WeightedFusion};

use crate::features::search::engine::hybrid::HybridSearchResult;

pub trait SearchFusionStrategy: Send + Sync {
    fn fuse(
        &self,
        vector_results: Vec<HybridSearchResult>,
        keyword_results: Vec<HybridSearchResult>,
    ) -> Vec<HybridSearchResult>;

    fn name(&self) -> &'static str;
}
