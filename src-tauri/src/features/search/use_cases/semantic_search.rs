//! # Semantic Search Use Case
//!
//! Performs vector-based semantic search using embeddings.
//!
//! This use case orchestrates:
//! 1. Query embedding generation
//! 2. Vector similarity search
//! 3. Result ranking and filtering
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::use_cases::search::semantic_search::SemanticSearchUseCase;
//! use lattice::application::dtos::search_dto::{SearchRequestDto, SearchModeDto};
//!
//! # async fn example(use_case: SemanticSearchUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = SearchRequestDto {
//!     query: "machine learning algorithms".to_string(),
//!     limit: Some(10),
//!     threshold: Some(0.7),
//!     mode: SearchModeDto::Vector,
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Found {} results in {}ms",
//!     response.total,
//!     response.query_time_ms
//! );
//! # Ok(())
//! # }
//! ```

use std::collections::HashSet;
use std::sync::Arc;

use crate::application::ports::{EmbeddingPort, VectorSearchPort};
use crate::features::search::dto::{SearchRequestDto, SearchResponseDto};
use crate::features::search::mapper::SearchMapper;
use crate::shared::error::{AppError, Result};

/// Semantic search use case.
///
///  Coordinates vector-based semantic search by generating query embeddings
/// and searching the vector index.
///
/// ## Dependencies
///
/// - `EmbeddingPort`: Generates embeddings for the search query
/// - `VectorSearchPort`: Performs similarity search in the vector index
pub struct SemanticSearchUseCase {
    embedding_service: Arc<dyn EmbeddingPort>,
    vector_search: Arc<dyn VectorSearchPort>,
    chunk_repository: Option<Arc<dyn crate::application::ports::ChunkRepositoryPort>>,
}

impl SemanticSearchUseCase {
    /// Create a new semantic search use case.
    ///
    /// # Arguments
    ///
    /// * `embedding_service` - Service for generating text embeddings
    /// * `vector_search` - Service for vector similarity search
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::application::use_cases::search::semantic_search::SemanticSearchUseCase;
    /// # use std::sync::Arc;
    /// # use lattice::application::ports::{EmbeddingPort, VectorSearchPort};
    ///
    /// # fn example(embedder: Arc<dyn EmbeddingPort>, searcher: Arc<dyn VectorSearchPort>) {
    /// let use_case = SemanticSearchUseCase::new(embedder, searcher);
    /// # }
    /// ```
    pub fn new(
        embedding_service: Arc<dyn EmbeddingPort>,
        vector_search: Arc<dyn VectorSearchPort>,
    ) -> Self {
        Self {
            embedding_service,
            vector_search,
            chunk_repository: None,
        }
    }

    #[must_use]
    pub fn with_chunk_repository(
        mut self,
        repository: Arc<dyn crate::application::ports::ChunkRepositoryPort>,
    ) -> Self {
        self.chunk_repository = Some(repository);
        self
    }

    /// Execute semantic search.
    ///
    /// # Arguments
    ///
    /// * `request` - Search request containing query and parameters
    ///
    /// # Returns
    ///
    /// Search response with ranked results and metadata
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Embedding generation fails
    /// - Vector search fails
    /// - Query is invalid
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::application::use_cases::search::semantic_search::SemanticSearchUseCase;
    /// # use lattice::application::dtos::search_dto::{SearchRequestDto, SearchModeDto};
    /// # async fn example(use_case: SemanticSearchUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = SearchRequestDto {
    ///     query: "semantic search".to_string(),
    ///     limit: Some(5),
    ///     threshold: Some(0.8),
    ///     mode: SearchModeDto::Vector,
    /// };
    ///
    /// let response = use_case.execute(request).await?;
    /// for result in response.results {
    ///     println!("Found: {} (score: {})", result.title, result.score);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(&self, request: SearchRequestDto) -> Result<SearchResponseDto> {
        self.execute_scoped(request, None).await
    }

    /// Execute semantic search with optional hard-scoped document allow-list.
    pub async fn execute_scoped(
        &self,
        request: SearchRequestDto,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<SearchResponseDto> {
        let start = std::time::Instant::now();

        // 1. Generate query embedding
        let query_embedding = self.embedding_service.embed_query(&request.query).await?;

        // 2. Perform vector search (returns port DTOs)
        let limit = request.limit.unwrap_or(10);
        let threshold = request.threshold.unwrap_or(0.5);

        // The USearch query is synchronous and CPU-bound. Run inline, it stalls
        // this runtime thread and the BM25 branch chat joins beside it, as
        // `HybridSearchUseCase::vector_branch` explains. The allow-list is a
        // set of document ids, cheap to copy next to the search itself.
        let vector_search = Arc::clone(&self.vector_search);
        let allowed = allowed_document_ids.cloned();
        let mut port_dtos = tokio::task::spawn_blocking(move || {
            vector_search.search_scoped(&query_embedding, limit, threshold, allowed.as_ref())
        })
        .await
        .map_err(|error| {
            AppError::InternalError(format!("Vector search task failed: {error}"))
        })??;

        if let Some(repository) = &self.chunk_repository {
            let ids = port_dtos
                .iter()
                .map(|result| result.chunk_id.clone())
                .collect::<Vec<_>>();
            let content = repository.find_content_by_ids(&ids).await?;
            for result in &mut port_dtos {
                if let Some(text) = content.get(&result.chunk_id) {
                    result.content.clone_from(text);
                }
            }
        }

        // 3. Map port DTOs to domain entities
        let results = SearchMapper::port_dtos_to_domain(port_dtos);

        // 4. Convert to response DTO
        let query_time_ms = start.elapsed().as_millis() as u64;
        let response = SearchMapper::to_response_dto(results, query_time_ms);

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::search::dto::SearchModeDto;
    use async_trait::async_trait;

    struct MockEmbedder;

    #[async_trait]
    impl EmbeddingPort for MockEmbedder {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3, 0.4]) // Mock embedding
        }

        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1, 0.2, 0.3, 0.4]]) // Mock embeddings
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }

        fn dimension(&self) -> usize {
            4
        }
    }

    struct MockVectorSearch;

    impl VectorSearchPort for MockVectorSearch {
        fn search(
            &self,
            _embedding: &[f32],
            limit: usize,
            _threshold: f32,
        ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
            Ok((0..limit.min(3))
                .map(|i| crate::features::search::dto::SearchResultPortDto {
                    doc_id: format!("doc-{}", i),
                    chunk_id: format!("chunk-{}", i),
                    score: 0.9 - (i as f32 * 0.1),
                    content: format!("Content {}", i),
                })
                .collect())
        }

        fn search_scoped(
            &self,
            query_embedding: &[f32],
            top_k: usize,
            threshold: f32,
            allowed_document_ids: Option<&std::collections::HashSet<String>>,
        ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
            let mut results = self.search(query_embedding, top_k, threshold)?;
            if let Some(scope) = allowed_document_ids {
                results.retain(|result| scope.contains(&result.doc_id));
                results.truncate(top_k);
            }
            Ok(results)
        }

        fn add_embedding(&self, _id: String, _embedding: Vec<f32>) -> Result<()> {
            Ok(())
        }

        fn remove_embedding(&self, _id: &str) -> Result<()> {
            Ok(())
        }

        fn clear(&self) -> Result<()> {
            Ok(())
        }

        fn count(&self) -> usize {
            10
        }

        fn dimension(&self) -> usize {
            4
        }
    }

    /// Records which thread ran the USearch call.
    #[derive(Default)]
    struct ThreadRecordingSearch {
        thread: std::sync::Mutex<Option<std::thread::ThreadId>>,
    }

    impl VectorSearchPort for ThreadRecordingSearch {
        fn search(
            &self,
            embedding: &[f32],
            limit: usize,
            threshold: f32,
        ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
            *self.thread.lock().unwrap() = Some(std::thread::current().id());
            MockVectorSearch.search(embedding, limit, threshold)
        }

        fn search_scoped(
            &self,
            query_embedding: &[f32],
            top_k: usize,
            threshold: f32,
            _allowed_document_ids: Option<&std::collections::HashSet<String>>,
        ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
            self.search(query_embedding, top_k, threshold)
        }

        fn add_embedding(&self, _id: String, _embedding: Vec<f32>) -> Result<()> {
            Ok(())
        }

        fn remove_embedding(&self, _id: &str) -> Result<()> {
            Ok(())
        }

        fn clear(&self) -> Result<()> {
            Ok(())
        }

        fn count(&self) -> usize {
            10
        }

        fn dimension(&self) -> usize {
            4
        }
    }

    #[tokio::test]
    async fn the_vector_search_runs_off_the_async_worker() {
        let search = Arc::new(ThreadRecordingSearch::default());
        let use_case = SemanticSearchUseCase::new(Arc::new(MockEmbedder), search.clone());
        let scope: HashSet<String> = ["doc-0".to_string()].into_iter().collect();

        use_case
            .execute_scoped(
                SearchRequestDto {
                    query: "q".to_string(),
                    limit: Some(3),
                    threshold: Some(0.0),
                    mode: SearchModeDto::Vector,
                },
                Some(&scope),
            )
            .await
            .unwrap();

        let ran_on = search.thread.lock().unwrap().expect("search ran");
        assert_ne!(ran_on, std::thread::current().id());
    }

    #[tokio::test]
    async fn test_semantic_search_execution() {
        let use_case =
            SemanticSearchUseCase::new(Arc::new(MockEmbedder), Arc::new(MockVectorSearch));

        let request = SearchRequestDto {
            query: "test query".to_string(),
            limit: Some(5),
            threshold: Some(0.7),
            mode: SearchModeDto::Vector,
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.results.len(), 3); // MockVectorSearch returns max 3
        assert_eq!(response.total, 3);
    }

    #[tokio::test]
    async fn test_semantic_search_with_defaults() {
        let use_case =
            SemanticSearchUseCase::new(Arc::new(MockEmbedder), Arc::new(MockVectorSearch));

        let request = SearchRequestDto {
            query: "test".to_string(),
            limit: None,     // Should default to 10
            threshold: None, // Should default to 0.5
            mode: SearchModeDto::Vector,
        };

        let response = use_case.execute(request).await.unwrap();

        assert!(!response.results.is_empty());
    }

    #[tokio::test]
    async fn test_semantic_search_result_ordering() {
        let use_case =
            SemanticSearchUseCase::new(Arc::new(MockEmbedder), Arc::new(MockVectorSearch));

        let request = SearchRequestDto {
            query: "test".to_string(),
            limit: Some(3),
            threshold: Some(0.0),
            mode: SearchModeDto::Vector,
        };

        let response = use_case.execute(request).await.unwrap();

        // Results should be ordered by score (descending)
        for i in 0..response.results.len() - 1 {
            assert!(response.results[i].score >= response.results[i + 1].score);
        }
    }
}
