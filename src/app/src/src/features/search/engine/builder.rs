use std::marker::PhantomData;
use std::sync::Arc;

use crate::infrastructure::search::bm25::BM25Search;
use crate::infrastructure::search::hybrid::HybridSearchService;
use crate::infrastructure::search::reranker::RerankerService;
use crate::infrastructure::services::traits::SearchEnrichmentServiceTrait;
use crate::features::search::{BM25SearchTrait, SearchServiceTrait};
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;

pub struct Uninitialized;
pub struct WithVector;
pub struct Ready;

pub struct HybridSearchBuilder<State = Uninitialized> {
    vector_search: Option<Arc<dyn SearchServiceTrait>>,
    bm25_search: Option<BM25Search>,
    pool: Option<SqlitePool>,
    enrichment: Option<Arc<dyn SearchEnrichmentServiceTrait>>,
    rrf_k: f32,
    reranker: Option<RerankerService>,
    _state: PhantomData<State>,
}

impl HybridSearchBuilder<Uninitialized> {
    pub fn new() -> Self {
        Self {
            vector_search: None,
            bm25_search: None,
            pool: None,
            enrichment: None,
            rrf_k: 60.0,
            reranker: None,
            _state: PhantomData,
        }
    }

    pub fn vector_search(
        self,
        vector_search: Arc<dyn SearchServiceTrait>,
    ) -> HybridSearchBuilder<WithVector> {
        HybridSearchBuilder {
            vector_search: Some(vector_search),
            bm25_search: self.bm25_search,
            pool: self.pool,
            enrichment: self.enrichment,
            rrf_k: self.rrf_k,
            reranker: self.reranker,
            _state: PhantomData,
        }
    }
}

impl Default for HybridSearchBuilder<Uninitialized> {
    fn default() -> Self {
        Self::new()
    }
}

impl HybridSearchBuilder<WithVector> {
    pub fn bm25_search(self, bm25_search: BM25Search) -> HybridSearchBuilder<Ready> {
        HybridSearchBuilder {
            vector_search: self.vector_search,
            bm25_search: Some(bm25_search),
            pool: self.pool,
            enrichment: self.enrichment,
            rrf_k: self.rrf_k,
            reranker: self.reranker,
            _state: PhantomData,
        }
    }
}

impl<State> HybridSearchBuilder<State> {
    pub fn pool(mut self, pool: SqlitePool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn enrichment(mut self, enrichment: Arc<dyn SearchEnrichmentServiceTrait>) -> Self {
        self.enrichment = Some(enrichment);
        self
    }

    pub fn rrf_k(mut self, k: f32) -> Self {
        self.rrf_k = k;
        self
    }

    pub fn reranker(mut self, reranker: RerankerService) -> Self {
        self.reranker = Some(reranker);
        self
    }
}

impl HybridSearchBuilder<Ready> {
    pub fn build(self) -> Result<HybridSearchService> {
        let vector_search = self.vector_search.ok_or_else(|| {
            AppError::InvalidConfig("Vector search not set in HybridSearchBuilder".to_string())
        })?;
        let bm25_search = self.bm25_search.ok_or_else(|| {
            AppError::InvalidConfig("BM25 search not set in HybridSearchBuilder".to_string())
        })?;
        let pool = self.pool.ok_or_else(|| {
            AppError::InvalidConfig("Database pool not set in HybridSearchBuilder".to_string())
        })?;
        let enrichment = self.enrichment.ok_or_else(|| {
            AppError::InvalidConfig(
                "Search enrichment service not set in HybridSearchBuilder".to_string(),
            )
        })?;

        let bm25_search: Arc<dyn BM25SearchTrait> = Arc::new(bm25_search);

        let mut service = HybridSearchService::with_rrf_k(
            vector_search,
            bm25_search,
            pool,
            enrichment,
            self.rrf_k,
        );

        if let Some(reranker) = self.reranker {
            service = service.with_reranker(reranker);
        }

        Ok(service)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_type_safety() {
        let _builder = HybridSearchBuilder::new();
    }
}
