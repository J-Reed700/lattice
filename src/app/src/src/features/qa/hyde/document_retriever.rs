//! # Document Retriever for HyDE System
//!
//! Phase 3: Hybrid search with parallel embedding generation.
//!
//! ## Overview
//!
//! The DocumentRetriever takes HyDEInterpretation from Phase 2 and performs:
//! 1. **Parallel embedding generation**: Generate embeddings for both raw query and HyDE text concurrently
//! 2. **Strategy-based search**: Execute search based on SearchStrategy (HyDEOnly, RawOnly, Hybrid)
//! 3. **Result merging**: Deduplicate and re-rank results for hybrid search
//!
//! ## Architecture
//!
//! ```text
//! HyDEInterpretation
//!       |
//!       v
//! DocumentRetriever
//!   |           |
//!   |           v
//!   |    Parallel Embedding Generation
//!   |     (raw query + HyDE text)
//!   |           |
//!   v           v
//! SearchStrategy Router
//!   |     |      |
//!   v     v      v
//! Raw  HyDE  Hybrid
//!   |     |      |
//!   +-----+------+
//!         |
//!         v
//!   Result Merger
//!         |
//!         v
//!  DocumentChunk[]
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vault_desktop::infrastructure::services::hyde::DocumentRetriever;
//! use vault_desktop::domain::qa::{HyDEInterpretation, SearchStrategy};
//!
//! let interpretation = HyDEInterpretation::for_question(
//!     "What is DDD?",
//!     "Domain-Driven Design is a software design approach..."
//! );
//!
//! let retriever = DocumentRetriever::new(embedding_service, search_service);
//! let documents = retriever.retrieve(&interpretation, 10).await?;
//!
//! assert!(!documents.is_empty());
//! ```

use crate::domain::qa::hyde::{DocumentChunk, HyDEInterpretation, SearchStrategy};
use crate::infrastructure::search::service::SearchResult;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::search::SearchServiceTrait;
use crate::shared::error::Result;
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Document Retriever
// ============================================================================

/// Retrieves documents using HyDE interpretations with parallel embedding generation.
///
/// Core component of Phase 3 that implements:
/// - Parallel embedding generation for raw query and HyDE text
/// - Strategy-based search routing
/// - Hybrid result merging with deduplication
pub struct DocumentRetriever {
    /// Embedding service for generating query embeddings
    embedding_service: Arc<dyn EmbeddingServiceTrait>,
    /// Search service for vector similarity search
    search_service: Arc<dyn SearchServiceTrait>,
}

impl DocumentRetriever {
    /// Create a new DocumentRetriever.
    ///
    /// # Arguments
    /// * `embedding_service` - Service for generating embeddings
    /// * `search_service` - Service for vector similarity search
    pub fn new(
        embedding_service: Arc<dyn EmbeddingServiceTrait>,
        search_service: Arc<dyn SearchServiceTrait>,
    ) -> Self {
        Self {
            embedding_service,
            search_service,
        }
    }

    /// Retrieve documents based on HyDE interpretation.
    ///
    /// Implements parallel embedding generation and strategy-based search.
    ///
    /// # Arguments
    /// * `interpretation` - HyDE interpretation with query classification and strategy
    /// * `top_k` - Maximum number of documents to retrieve
    ///
    /// # Returns
    /// Vector of DocumentChunk ordered by relevance (highest score first)
    ///
    /// # Example
    /// ```rust,no_run
    /// let interpretation = HyDEInterpretation::for_question("What is Rust?", "Rust is...");
    /// let documents = retriever.retrieve(&interpretation, 10).await?;
    /// ```
    pub async fn retrieve(
        &self,
        interpretation: &HyDEInterpretation,
        top_k: usize,
    ) -> Result<Vec<DocumentChunk>> {
        match interpretation.search_strategy {
            SearchStrategy::HyDEOnly => self.search_hyde_only(interpretation, top_k).await,
            SearchStrategy::RawOnly => self.search_raw_only(interpretation, top_k).await,
            SearchStrategy::Hybrid => self.search_hybrid(interpretation, top_k).await,
        }
    }

    /// Search using only the HyDE-generated hypothetical answer.
    ///
    /// Best for factual questions where HyDE can generate a semantically similar answer.
    async fn search_hyde_only(
        &self,
        interpretation: &HyDEInterpretation,
        top_k: usize,
    ) -> Result<Vec<DocumentChunk>> {
        if let Some(hyde_text) = &interpretation.hyde_text {
            let embedding = self.embedding_service.embed_single(hyde_text).await?;
            let results = self.search_service.search(&embedding, top_k)?;
            Ok(self.convert_results_to_chunks(results))
        } else {
            tracing::warn!(
                "HyDEOnly strategy requested but no HyDE text available, falling back to raw query"
            );
            self.search_raw_only(interpretation, top_k).await
        }
    }

    /// Search using only the original raw query.
    ///
    /// Fast path for greetings, commands, and queries that don't benefit from HyDE.
    async fn search_raw_only(
        &self,
        interpretation: &HyDEInterpretation,
        top_k: usize,
    ) -> Result<Vec<DocumentChunk>> {
        let embedding = self
            .embedding_service
            .embed_single(&interpretation.original_query)
            .await?;
        let results = self.search_service.search(&embedding, top_k)?;
        Ok(self.convert_results_to_chunks(results))
    }

    /// Search using both HyDE and raw query, then merge results.
    ///
    /// Parallel embedding generation and hybrid result fusion:
    /// - 70% weight for HyDE results
    /// - 30% weight for raw query results
    /// - Deduplication by document ID
    /// - Re-ranking by combined score
    async fn search_hybrid(
        &self,
        interpretation: &HyDEInterpretation,
        top_k: usize,
    ) -> Result<Vec<DocumentChunk>> {
        // Parallel embedding generation
        let (hyde_results, raw_results) = if let Some(hyde_text) = &interpretation.hyde_text {
            let raw_query = interpretation.original_query.clone();
            let hyde_text_clone = hyde_text.clone();

            // Generate embeddings in parallel
            let (hyde_embedding, raw_embedding) = tokio::join!(
                self.embedding_service.embed_single(&hyde_text_clone),
                self.embedding_service.embed_single(&raw_query)
            );

            // Search with both embeddings in parallel
            let hyde_embedding = hyde_embedding?;
            let raw_embedding = raw_embedding?;

            let hyde_search = self.search_service.search(&hyde_embedding, top_k)?;
            let raw_search = self.search_service.search(&raw_embedding, top_k)?;

            (hyde_search, raw_search)
        } else {
            // Fallback if no HyDE text
            tracing::warn!("Hybrid strategy requested but no HyDE text, using raw query only");
            return self.search_raw_only(interpretation, top_k).await;
        };

        // Merge and deduplicate results
        Ok(self.merge_hybrid_results(hyde_results, raw_results, top_k))
    }

    /// Merge hybrid search results with weighted scoring and deduplication.
    ///
    /// Algorithm:
    /// 1. Apply weights: 70% HyDE, 30% raw
    /// 2. Deduplicate by document ID (keep highest score)
    /// 3. Re-rank by combined weighted score
    /// 4. Return top_k results
    fn merge_hybrid_results(
        &self,
        hyde_results: Vec<SearchResult>,
        raw_results: Vec<SearchResult>,
        top_k: usize,
    ) -> Vec<DocumentChunk> {
        const HYDE_WEIGHT: f32 = 0.7;
        const RAW_WEIGHT: f32 = 0.3;

        // Build combined score map
        let mut score_map: HashMap<String, (SearchResult, f32)> = HashMap::new();

        // Add HyDE results with 70% weight
        for result in hyde_results {
            let weighted_score = result.score * HYDE_WEIGHT;
            score_map.insert(result.id.clone(), (result, weighted_score));
        }

        // Add raw results with 30% weight, combining if already present
        for result in raw_results {
            let weighted_score = result.score * RAW_WEIGHT;

            score_map
                .entry(result.id.clone())
                .and_modify(|(_, existing_score)| {
                    *existing_score += weighted_score;
                })
                .or_insert((result, weighted_score));
        }

        // Convert to vector and sort by combined score
        let mut combined: Vec<(SearchResult, f32)> = score_map.into_values().collect();
        combined.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top_k and convert to DocumentChunk
        combined
            .into_iter()
            .take(top_k)
            .map(|(result, score)| self.convert_result_to_chunk(result, score))
            .collect()
    }

    /// Convert search results to document chunks.
    fn convert_results_to_chunks(&self, results: Vec<SearchResult>) -> Vec<DocumentChunk> {
        results
            .into_iter()
            .map(|r| {
                let score = r.score;
                self.convert_result_to_chunk(r, score)
            })
            .collect()
    }

    /// Convert a single search result to a document chunk.
    fn convert_result_to_chunk(&self, result: SearchResult, score: f32) -> DocumentChunk {
        DocumentChunk {
            content: result.content.unwrap_or_else(|| result.id.clone()),
            file_path: result
                .file_path
                .or(result.filename)
                .unwrap_or_else(|| format!("unknown_{}", result.id)),
            similarity_score: score,
            chunk_index: result.chunk_index.unwrap_or(0),
            total_chunks: 1,
            metadata: None,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::embedding::EmbeddingServiceTrait;
    use async_trait::async_trait;
    use std::sync::Mutex;

    // Mock embedding service
    struct MockEmbeddingService {
        embeddings: Mutex<HashMap<String, Vec<f32>>>,
    }

    impl MockEmbeddingService {
        fn new() -> Self {
            Self {
                embeddings: Mutex::new(HashMap::new()),
            }
        }

        fn set_embedding(&self, text: &str, embedding: Vec<f32>) {
            self.embeddings
                .lock()
                .unwrap()
                .insert(text.to_string(), embedding);
        }
    }

    #[async_trait]
    impl EmbeddingServiceTrait for MockEmbeddingService {
        async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
            Ok(self
                .embeddings
                .lock()
                .unwrap()
                .get(text)
                .cloned()
                .unwrap_or_else(|| vec![0.0, 0.0, 0.0]))
        }

        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(vec![])
        }

        async fn embed_contextualized_chunks(
            &self,
            _chunks: &[crate::infrastructure::indexing::chunker::ContextualizedChunk],
        ) -> Result<Vec<Vec<f32>>> {
            Ok(vec![])
        }
    }

    // Mock search service
    struct MockSearchService {
        results: Mutex<HashMap<String, Vec<SearchResult>>>,
    }

    impl MockSearchService {
        fn new() -> Self {
            Self {
                results: Mutex::new(HashMap::new()),
            }
        }

        fn set_results(&self, embedding_key: &str, results: Vec<SearchResult>) {
            self.results
                .lock()
                .unwrap()
                .insert(embedding_key.to_string(), results);
        }

        fn embedding_to_key(embedding: &[f32]) -> String {
            format!("{:?}", embedding)
        }
    }

    #[async_trait]
    impl SearchServiceTrait for MockSearchService {
        fn search(&self, query_embedding: &[f32], _top_k: usize) -> Result<Vec<SearchResult>> {
            let key = Self::embedding_to_key(query_embedding);
            Ok(self
                .results
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .unwrap_or_default())
        }

        async fn search_with_metadata(
            &self,
            _query_embedding: &[f32],
            _top_k: usize,
        ) -> Result<Vec<SearchResult>> {
            Ok(vec![])
        }

        fn search_with_threshold(
            &self,
            _query_embedding: &[f32],
            _top_k: usize,
            _threshold: f32,
        ) -> Result<Vec<SearchResult>> {
            Ok(vec![])
        }

        fn batch_search(
            &self,
            _query_embeddings: &[Vec<f32>],
            _top_k: usize,
        ) -> Result<Vec<Vec<SearchResult>>> {
            Ok(vec![])
        }
    }

    fn create_search_result(id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            score,
            index: 0,
            filename: Some(format!("{}.txt", id)),
            mime_type: Some("text/plain".to_string()),
            size_bytes: Some(1024),
            created_at: None,
            content: Some(format!("Content of {}", id)),
            file_id: None,
            file_path: Some(format!("/path/{}.txt", id)),
            file_name: None,
            file_extension: None,
            file_category: None,
            is_indexed: None,
            document_id: None,
            snippet: None,
            chunk_index: Some(0),
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn test_search_hyde_only() {
        let embedding_service = Arc::new(MockEmbeddingService::new());
        let search_service = Arc::new(MockSearchService::new());

        // Setup
        let hyde_embedding = vec![1.0, 0.0, 0.0];
        embedding_service.set_embedding("DDD is a design approach", hyde_embedding.clone());

        let results = vec![
            create_search_result("doc1", 0.95),
            create_search_result("doc2", 0.85),
        ];
        search_service.set_results(
            &MockSearchService::embedding_to_key(&hyde_embedding),
            results,
        );

        let retriever = DocumentRetriever::new(embedding_service, search_service);

        let interpretation =
            HyDEInterpretation::for_question("What is DDD?", "DDD is a design approach");

        let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

        assert_eq!(documents.len(), 2);
        assert_eq!(documents[0].similarity_score, 0.95);
        assert_eq!(documents[1].similarity_score, 0.85);
    }

    #[tokio::test]
    async fn test_search_raw_only() {
        let embedding_service = Arc::new(MockEmbeddingService::new());
        let search_service = Arc::new(MockSearchService::new());

        // Setup
        let raw_embedding = vec![0.0, 1.0, 0.0];
        embedding_service.set_embedding("Hello", raw_embedding.clone());

        let results = vec![create_search_result("greeting_doc", 0.80)];
        search_service.set_results(
            &MockSearchService::embedding_to_key(&raw_embedding),
            results,
        );

        let retriever = DocumentRetriever::new(embedding_service, search_service);

        let interpretation = HyDEInterpretation::for_greeting("Hello");

        let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].similarity_score, 0.80);
    }

    #[tokio::test]
    async fn test_search_hybrid_parallel_generation() {
        let embedding_service = Arc::new(MockEmbeddingService::new());
        let search_service = Arc::new(MockSearchService::new());

        // Setup embeddings
        let hyde_embedding = vec![1.0, 0.0, 0.0];
        let raw_embedding = vec![0.0, 1.0, 0.0];

        embedding_service.set_embedding("Rust is a programming language", hyde_embedding.clone());
        embedding_service.set_embedding("What is Rust?", raw_embedding.clone());

        // Setup search results
        let hyde_results = vec![
            create_search_result("doc1", 0.95),
            create_search_result("doc2", 0.85),
        ];
        let raw_results = vec![
            create_search_result("doc2", 0.75), // Duplicate
            create_search_result("doc3", 0.70),
        ];

        search_service.set_results(
            &MockSearchService::embedding_to_key(&hyde_embedding),
            hyde_results,
        );
        search_service.set_results(
            &MockSearchService::embedding_to_key(&raw_embedding),
            raw_results,
        );

        let retriever = DocumentRetriever::new(embedding_service, search_service);

        let interpretation = HyDEInterpretation::hybrid(
            "What is Rust?",
            "Rust is a programming language",
            crate::domain::qa::hyde::QueryType::Question,
        );

        let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

        // Should have 3 unique documents (doc1, doc2, doc3)
        assert_eq!(documents.len(), 3);

        // doc1: 0.95 * 0.7 = 0.665 (HyDE only)
        // doc2: 0.85 * 0.7 + 0.75 * 0.3 = 0.595 + 0.225 = 0.82 (combined)
        // doc3: 0.70 * 0.3 = 0.21 (raw only)

        // doc2 should be first (highest combined score)
        assert!(documents[0].file_path.contains("doc2"));
        assert!((documents[0].similarity_score - 0.82).abs() < 0.01);

        // doc1 should be second
        assert!(documents[1].file_path.contains("doc1"));
        assert!((documents[1].similarity_score - 0.665).abs() < 0.01);

        // doc3 should be third
        assert!(documents[2].file_path.contains("doc3"));
        assert!((documents[2].similarity_score - 0.21).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_merge_hybrid_results_deduplication() {
        let embedding_service = Arc::new(MockEmbeddingService::new());
        let search_service = Arc::new(MockSearchService::new());

        let retriever = DocumentRetriever::new(embedding_service, search_service);

        let hyde_results = vec![
            create_search_result("doc1", 0.90),
            create_search_result("doc2", 0.85),
        ];

        let raw_results = vec![
            create_search_result("doc2", 0.80), // Duplicate
            create_search_result("doc3", 0.75),
        ];

        let merged = retriever.merge_hybrid_results(hyde_results, raw_results, 10);

        // Should have 3 unique documents
        assert_eq!(merged.len(), 3);

        // Verify doc2 has combined score
        let doc2 = merged
            .iter()
            .find(|d| d.file_path.contains("doc2"))
            .unwrap();
        // 0.85 * 0.7 + 0.80 * 0.3 = 0.595 + 0.24 = 0.835
        assert!((doc2.similarity_score - 0.835).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_merge_hybrid_results_respects_top_k() {
        let embedding_service = Arc::new(MockEmbeddingService::new());
        let search_service = Arc::new(MockSearchService::new());

        let retriever = DocumentRetriever::new(embedding_service, search_service);

        let hyde_results = vec![
            create_search_result("doc1", 0.95),
            create_search_result("doc2", 0.90),
            create_search_result("doc3", 0.85),
        ];

        let raw_results = vec![
            create_search_result("doc4", 0.80),
            create_search_result("doc5", 0.75),
        ];

        let merged = retriever.merge_hybrid_results(hyde_results, raw_results, 3);

        // Should only return top 3
        assert_eq!(merged.len(), 3);

        // Should be ordered by combined score
        assert!(merged[0].similarity_score >= merged[1].similarity_score);
        assert!(merged[1].similarity_score >= merged[2].similarity_score);
    }

    #[tokio::test]
    async fn test_hyde_only_fallback_to_raw() {
        let embedding_service = Arc::new(MockEmbeddingService::new());
        let search_service = Arc::new(MockSearchService::new());

        // Setup raw embedding
        let raw_embedding = vec![0.0, 1.0, 0.0];
        embedding_service.set_embedding("test query", raw_embedding.clone());

        let results = vec![create_search_result("doc1", 0.80)];
        search_service.set_results(
            &MockSearchService::embedding_to_key(&raw_embedding),
            results,
        );

        let retriever = DocumentRetriever::new(embedding_service, search_service);

        // Create interpretation with HyDEOnly strategy but no HyDE text
        let interpretation = HyDEInterpretation {
            query_type: crate::domain::qa::hyde::QueryType::Question,
            original_query: "test query".to_string(),
            hyde_text: None,
            search_strategy: SearchStrategy::HyDEOnly,
            tool_intent: crate::domain::qa::hyde::ToolIntent::default_for_query_type(
                crate::domain::qa::hyde::QueryType::Question,
            ),
        };

        let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

        // Should fall back to raw query
        assert_eq!(documents.len(), 1);
    }
}
