//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use super::trait_def::{BM25SearchTrait, SearchServiceTrait};
#[cfg(test)]
use crate::features::search::engine::service::SearchResult;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use lattice_core::shared::error::Result;
#[cfg(test)]
use std::sync::{Arc, RwLock};

#[cfg(test)]
/// Mock search service with in-memory index
///
/// Stores embeddings in memory and performs brute-force cosine similarity search.
pub struct MockSearchService {
    embeddings: Vec<(String, Vec<f32>)>, // (id, embedding) pairs
}

#[cfg(test)]
impl MockSearchService {
    pub fn new() -> Self {
        Self {
            embeddings: Vec::new(),
        }
    }

    /// Add an embedding to the index
    pub fn add_embedding(&mut self, id: String, embedding: Vec<f32>) {
        self.embeddings.push((id, embedding));
    }

    /// Add multiple embeddings
    pub fn add_embeddings(&mut self, embeddings: Vec<(String, Vec<f32>)>) {
        self.embeddings.extend(embeddings);
    }

    /// Clear all embeddings
    pub fn clear(&mut self) {
        self.embeddings.clear();
    }

    /// Compute cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if mag_a == 0.0 || mag_b == 0.0 {
            0.0
        } else {
            dot / (mag_a * mag_b)
        }
    }
}

#[cfg(test)]
impl Default for MockSearchService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl SearchServiceTrait for MockSearchService {
    fn search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        if self.embeddings.is_empty() {
            return Ok(vec![]);
        }

        let mut scores: Vec<(usize, f32, &str)> = self
            .embeddings
            .iter()
            .enumerate()
            .map(|(idx, (id, emb))| {
                let score = Self::cosine_similarity(query_embedding, emb);
                (idx, score, id.as_str())
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);

        Ok(scores
            .into_iter()
            .map(|(idx, score, id)| SearchResult {
                id: id.to_string(),
                score,
                index: idx,
                filename: None,
                mime_type: None,
                size_bytes: None,
                created_at: None,
                content: None,
                file_id: None,
                file_path: None,
                file_name: None,
                file_extension: None,
                file_category: None,
                is_indexed: None,
                document_id: None,
                snippet: None,
                chunk_index: None,
                updated_at: None,
            })
            .collect())
    }

    async fn search_with_metadata(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>> {
        self.search(query_embedding, top_k)
    }

    fn search_with_threshold(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResult>> {
        let results = self.search(query_embedding, top_k)?;
        Ok(results
            .into_iter()
            .filter(|r| r.score >= threshold)
            .collect())
    }

    fn batch_search(
        &self,
        query_embeddings: &[Vec<f32>],
        top_k: usize,
    ) -> Result<Vec<Vec<SearchResult>>> {
        query_embeddings
            .iter()
            .map(|query| self.search(query, top_k))
            .collect()
    }
}

#[cfg(test)]
/// Mock BM25 search service for testing
///
/// Stores query results in memory for deterministic testing.
/// Useful for testing without database dependencies.
pub struct MockBM25Search {
    results: Arc<
        RwLock<
            std::collections::HashMap<
                String,
                Vec<crate::features::search::engine::bm25::BM25Result>,
            >,
        >,
    >,
}

#[cfg(test)]
impl MockBM25Search {
    pub fn new() -> Self {
        Self {
            results: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Set results for a specific query
    ///
    /// # Example
    /// ```rust
    /// let mock = MockBM25Search::new();
    /// mock.set_results("rust", vec![
    ///     BM25Result {
    ///         document_id: "doc1".into(),
    ///         score: 10.5,
    ///         filename: Some("rust.txt".into()),
    ///         mime_type: Some("text/plain".into()),
    ///         size_bytes: Some(1024),
    ///         created_at: Some("2024-01-01".into()),
    ///         content: Some("Rust programming".into()),
    ///     }
    /// ]);
    /// ```
    pub fn set_results(
        &self,
        query: &str,
        results: Vec<crate::features::search::engine::bm25::BM25Result>,
    ) {
        self.results
            .write()
            .unwrap()
            .insert(query.to_string(), results);
    }

    /// Clear all stored results
    pub fn clear(&self) {
        self.results.write().unwrap().clear();
    }
}

#[cfg(test)]
impl Default for MockBM25Search {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl BM25SearchTrait for MockBM25Search {
    async fn search(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<crate::features::search::engine::bm25::BM25Result>> {
        Ok(self
            .results
            .read()
            .unwrap()
            .get(query)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(top_k)
            .collect())
    }

    async fn search_with_filter(
        &self,
        query: &str,
        top_k: usize,
        min_score: f32,
    ) -> Result<Vec<crate::features::search::engine::bm25::BM25Result>> {
        let all_results = self.search(query, top_k * 2).await?;
        Ok(all_results
            .into_iter()
            .filter(|r| r.score >= min_score)
            .take(top_k)
            .collect())
    }

    async fn optimize_index(&self) -> Result<()> {
        Ok(())
    }

    async fn rebuild_index(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
/// A keyword index with nothing in it.
pub struct EmptyTextSearch;

#[async_trait]
#[cfg(test)]
impl lattice_core::application::ports::TextSearchPort for EmptyTextSearch {
    async fn search(
        &self,
        _query: &str,
        _top_k: usize,
    ) -> Result<Vec<lattice_core::application::contracts::search::SearchResultRecord>> {
        Ok(Vec::new())
    }
    async fn index_document(&self, _id: &str, _content: &str) -> Result<()> {
        Ok(())
    }
    async fn index_batch(&self, _documents: &[(&str, &str)]) -> Result<()> {
        Ok(())
    }
    async fn remove_document(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    async fn clear(&self) -> Result<()> {
        Ok(())
    }
    async fn count(&self) -> Result<usize> {
        Ok(0)
    }
}

#[cfg(test)]
/// The library search orchestrator over an empty library, with no embedding
/// model loaded.
pub fn empty_hybrid_search() -> Arc<crate::features::search::use_cases::HybridSearchUseCase> {
    Arc::new(
        crate::features::search::use_cases::HybridSearchUseCase::new(
            Arc::new(lattice_core::application::ports::MockEmbeddingPort::new_degraded()),
            Arc::new(
                crate::features::search::engine::vector_search::USearchVectorIndex::new(4, None)
                    .expect("an in-memory vector index"),
            ),
            Arc::new(EmptyTextSearch),
        ),
    )
}

#[cfg(test)]
mod enrichment;
#[cfg(test)]
pub use enrichment::MockSearchEnrichmentService;

#[cfg(test)]
mod tests;
