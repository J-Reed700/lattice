//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::infrastructure::search::service::SearchResult;
#[cfg(test)]
use super::trait_def::{BM25SearchTrait, HybridSearchTrait, SearchServiceTrait};
#[cfg(test)]
use crate::shared::error::{AppError, Result};
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, Mutex, RwLock};

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
        // Mock doesn't have database access, so just return basic results
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
    results: Arc<RwLock<std::collections::HashMap<String, Vec<crate::search::bm25::BM25Result>>>>,
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
    pub fn set_results(&self, query: &str, results: Vec<crate::search::bm25::BM25Result>) {
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
    ) -> Result<Vec<crate::search::bm25::BM25Result>> {
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
    ) -> Result<Vec<crate::search::bm25::BM25Result>> {
        let all_results = self.search(query, top_k * 2).await?;
        Ok(all_results
            .into_iter()
            .filter(|r| r.score >= min_score)
            .take(top_k)
            .collect())
    }

    async fn optimize_index(&self) -> Result<()> {
        // Mock doesn't need to do anything
        Ok(())
    }

    async fn rebuild_index(&self) -> Result<()> {
        // Mock doesn't need to do anything
        Ok(())
    }
}

// ============================================================================
// Mock Service Implementations
// ============================================================================

#[cfg(test)]
/// Mock hybrid search service for testing
///
/// Simulates hybrid search without actual vector or BM25 search.
/// Returns configurable mock results for testing search flow.
/// All operations are deterministic and thread-safe.
pub struct MockHybridSearch {
    /// Mock results to return (query -> results mapping)
    results: Arc<
        RwLock<std::collections::HashMap<String, Vec<crate::search::hybrid::HybridSearchResult>>>,
    >,
}

#[cfg(test)]
impl MockHybridSearch {
    /// Create new mock hybrid search service
    ///
    /// # Returns
    /// New mock service with empty result set
    pub fn new() -> Self {
        Self {
            results: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Set mock results for a specific query
    ///
    /// # Arguments
    /// * `query` - Query text to match
    /// * `results` - Mock results to return
    ///
    /// # Example
    /// ```rust
    /// let mock = MockHybridSearch::new();
    /// mock.set_results("test query", vec![
    ///     HybridSearchResult {
    ///         id: "doc1".to_string(),
    ///         score: 0.95,
    ///         vector_score: Some(0.9),
    ///         bm25_score: Some(1.0),
    ///         vector_rank: Some(0),
    ///         bm25_rank: Some(0),
    ///         rerank_score: None,
    ///     },
    /// ]);
    /// ```
    pub fn set_results(
        &self,
        query: &str,
        results: Vec<crate::search::hybrid::HybridSearchResult>,
    ) {
        self.results
            .write()
            .unwrap()
            .insert(query.to_string(), results);
    }

    /// Clear all configured mock results
    pub fn clear(&self) {
        self.results.write().unwrap().clear();
    }
}

#[cfg(test)]
impl Default for MockHybridSearch {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl HybridSearchTrait for MockHybridSearch {
    async fn search(
        &self,
        query_text: &str,
        _query_embedding: &[f32],
        top_k: usize,
        _mode: crate::search::hybrid::SearchMode,
    ) -> Result<Vec<crate::search::hybrid::HybridSearchResult>> {
        Ok(self
            .results
            .read()
            .unwrap()
            .get(query_text)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(top_k)
            .collect())
    }

    async fn batch_search(
        &self,
        queries: Vec<(String, Vec<f32>)>,
        top_k: usize,
        mode: crate::search::hybrid::SearchMode,
    ) -> Result<Vec<Vec<crate::search::hybrid::HybridSearchResult>>> {
        let mut results = Vec::new();
        for (query_text, query_embedding) in queries {
            let result = self
                .search(&query_text, &query_embedding, top_k, mode)
                .await?;
            results.push(result);
        }
        Ok(results)
    }

    async fn search_with_recency(
        &self,
        query_text: &str,
        query_embedding: &[f32],
        top_k: usize,
        _recency_weight: f32,
        _max_age_days: i64,
    ) -> Result<Vec<crate::search::hybrid::HybridSearchResult>> {
        self.search(
            query_text,
            query_embedding,
            top_k,
            crate::search::hybrid::SearchMode::Hybrid,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::conversation::ConversationServiceTrait;
    use crate::features::conversation::mocks::MockConversationService;
    use crate::features::embedding::EmbeddingServiceTrait;
    use crate::features::embedding::mocks::MockEmbeddingService;
    use crate::features::tags::TagServiceTrait;
    use crate::features::tags::mocks::MockTagService;
    use crate::features::web::{WebIngestionResult, WebIngestionServiceTrait};
    use crate::features::web::mocks::MockWebIngestionService;
    use crate::infrastructure::services::traits::{
        FileStorageServiceTrait, ModelManagerTrait, SearchEnrichmentServiceTrait,
    };
    use crate::infrastructure::services::mocks::{
        MockFileStorageService, MockModelManager, MockSearchEnrichmentService,
    };

    #[tokio::test]
    async fn test_mock_embedding_service() {
        let service = MockEmbeddingService::new(384);

        let embedding = service.embed_single("hello world").await.unwrap();
        assert_eq!(embedding.len(), 384);

        // Same text should produce same embedding (deterministic)
        let embedding2 = service.embed_single("hello world").await.unwrap();
        assert_eq!(embedding, embedding2);

        // Different text should produce different embedding
        let embedding3 = service.embed_single("goodbye world").await.unwrap();
        assert_ne!(embedding, embedding3);
    }

    #[tokio::test]
    async fn test_mock_embedding_batch() {
        let service = MockEmbeddingService::new(384);

        let texts = vec!["hello".to_string(), "world".to_string()];
        let embeddings = service.embed_batch(&texts).await.unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 384);
        assert_eq!(embeddings[1].len(), 384);
    }

    #[tokio::test]
    async fn test_mock_search_service() {
        let mut service = MockSearchService::new();

        // Add some test embeddings
        service.add_embedding("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        service.add_embedding("doc2".to_string(), vec![0.0, 1.0, 0.0]);
        service.add_embedding("doc3".to_string(), vec![0.0, 0.0, 1.0]);

        // Search for doc1
        let query = vec![1.0, 0.0, 0.0];
        let results = service.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "doc1");
        assert!((results[0].score - 1.0).abs() < 1e-5);
    }

    #[tokio::test]
    async fn test_mock_search_with_threshold() {
        let mut service = MockSearchService::new();

        service.add_embedding("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        service.add_embedding("doc2".to_string(), vec![0.0, 1.0, 0.0]);

        let query = vec![1.0, 0.0, 0.0];
        let results = service.search_with_threshold(&query, 10, 0.9).unwrap();

        // Only doc1 should match with >0.9 similarity
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc1");
    }

    #[tokio::test]
    async fn test_mock_batch_search() {
        let mut service = MockSearchService::new();

        service.add_embedding("doc1".to_string(), vec![1.0, 0.0]);
        service.add_embedding("doc2".to_string(), vec![0.0, 1.0]);

        let queries = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let results = service.batch_search(&queries, 1).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0][0].id, "doc1");
        assert_eq!(results[1][0].id, "doc2");
    }

    #[tokio::test]
    async fn test_mock_tag_service() {
        let service = MockTagService::new();

        let tag = service.get_or_create("rust", "#ff5733").await.unwrap();
        assert_eq!(tag.name().as_str(), "rust");
        assert_eq!(tag.color(), "#ff5733");

        // Getting again should return same tag
        let tag2 = service.get_or_create("rust", "#000000").await.unwrap();
        assert_eq!(tag.id(), tag2.id());
        assert_eq!(tag2.color(), "#ff5733"); // Original color preserved
    }

    #[tokio::test]
    async fn test_mock_file_storage() {
        use crate::shared::domain_types::ValidatedFilePath;
        use std::path::PathBuf;

        let service = MockFileStorageService::new();

        let path = PathBuf::from("/test/file.txt");
        let validated_path = ValidatedFilePath::new(path.clone()).unwrap();
        let record = service
            .store_file(validated_path, "text/plain", None)
            .await
            .unwrap();

        assert!(!record.id.is_empty());
        assert_eq!(record.mime_type, "text/plain");

        // Retrieve path
        let retrieved_path = service.get_file_path(&record.id).await.unwrap();
        assert_eq!(retrieved_path, path);
    }

    #[tokio::test]
    async fn test_mock_model_manager_defaults() {
        let manager = MockModelManager::new();

        // By default, models should be ready
        assert!(manager.is_model_ready().await);
        assert!(manager.is_reranker_ready().await);
        assert_eq!(manager.get_download_progress(), Some(1.0));

        let info = manager.get_model_info();
        assert_eq!(info.name, "mock-model");
        assert!(info.is_downloaded);
    }

    #[tokio::test]
    async fn test_mock_model_manager_ensure_model() {
        let manager = MockModelManager::new();

        // Initially not ready
        manager.set_model_ready(false);
        assert!(!manager.is_model_ready().await);

        // Ensure makes it ready
        let path = manager.ensure_model_available().await.unwrap();
        assert!(manager.is_model_ready().await);
        assert_eq!(path, std::path::PathBuf::from("/mock/models/model.onnx"));
    }

    #[tokio::test]
    async fn test_mock_model_manager_ensure_reranker() {
        let manager = MockModelManager::new();

        // Initially not ready
        manager.set_reranker_ready(false);
        assert!(!manager.is_reranker_ready().await);

        // Ensure makes it ready
        let path = manager.ensure_reranker_available().await.unwrap();
        assert!(manager.is_reranker_ready().await);
        assert_eq!(
            path,
            std::path::PathBuf::from("/mock/models/reranker/model.safetensors")
        );
    }

    #[tokio::test]
    async fn test_mock_model_manager_paths() {
        let manager = MockModelManager::new();

        let model_path = manager.get_model_path();
        let tokenizer_path = manager.get_tokenizer_path();
        let reranker_path = manager.get_reranker_path();
        let reranker_tokenizer_path = manager.get_reranker_tokenizer_path();

        assert_eq!(
            model_path,
            std::path::PathBuf::from("/mock/models/model.onnx")
        );
        assert_eq!(
            tokenizer_path,
            std::path::PathBuf::from("/mock/models/tokenizer.json")
        );
        assert_eq!(
            reranker_path,
            std::path::PathBuf::from("/mock/models/reranker/model.safetensors")
        );
        assert_eq!(
            reranker_tokenizer_path,
            std::path::PathBuf::from("/mock/models/reranker/tokenizer.json")
        );
    }

    #[tokio::test]
    async fn test_mock_model_manager_custom_dir() {
        let custom_dir = std::path::PathBuf::from("/custom/path");
        let manager = MockModelManager::with_model_dir(custom_dir.clone());

        assert_eq!(manager.get_model_path(), custom_dir.join("model.onnx"));
        assert_eq!(
            manager.get_tokenizer_path(),
            custom_dir.join("tokenizer.json")
        );
    }

    #[tokio::test]
    async fn test_mock_model_manager_download_progress() {
        let manager = MockModelManager::new();

        // Default is complete
        assert_eq!(manager.get_download_progress(), Some(1.0));

        // Simulate in-progress download
        manager.set_download_progress(Some(0.5));
        assert_eq!(manager.get_download_progress(), Some(0.5));

        // Simulate not downloading
        manager.set_download_progress(None);
        assert_eq!(manager.get_download_progress(), None);
    }

    #[tokio::test]
    async fn test_mock_model_manager_cancel_download() {
        let manager = MockModelManager::new();

        manager.set_download_progress(Some(0.3));
        assert_eq!(manager.get_download_progress(), Some(0.3));

        // Cancel should clear progress
        manager.cancel_download();
        assert_eq!(manager.get_download_progress(), None);
    }

    #[tokio::test]
    async fn test_mock_model_manager_state_isolation() {
        let manager = MockModelManager::new();

        // Model and reranker states are independent
        manager.set_model_ready(true);
        manager.set_reranker_ready(false);

        assert!(manager.is_model_ready().await);
        assert!(!manager.is_reranker_ready().await);

        // Flip states
        manager.set_model_ready(false);
        manager.set_reranker_ready(true);

        assert!(!manager.is_model_ready().await);
        assert!(manager.is_reranker_ready().await);
    }

    #[tokio::test]
    async fn test_mock_web_ingestion_service_default() {
        let service = MockWebIngestionService::new();

        // Ingest a URL without configuration
        let result = service.ingest_url("https://example.com").await.unwrap();

        // Verify default result
        assert!(!result.document_id.is_empty());
        assert_eq!(result.url, "https://example.com");
        assert_eq!(result.title, "Mock Web Document");
        assert_eq!(result.word_count, 250);
        assert_eq!(result.chunks_created, 5);
        assert_eq!(result.site_name, Some("Mock Site".to_string()));
        assert_eq!(result.author, Some("Mock Author".to_string()));
        assert_eq!(result.reading_time_minutes, Some(2));

        // Verify tracking
        let urls = service.get_ingested_urls();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com");
    }

    #[tokio::test]
    async fn test_mock_web_ingestion_service_configured() {
        let service = MockWebIngestionService::new();

        // Configure custom result
        let custom_result = WebIngestionResult {
            document_id: "custom-id".to_string(),
            url: "https://test.com".to_string(),
            title: "Custom Title".to_string(),
            word_count: 500,
            chunks_created: 10,
            site_name: Some("Test Site".to_string()),
            author: Some("Test Author".to_string()),
            reading_time_minutes: Some(3),
        };
        service.set_result_for_url("https://test.com", custom_result.clone());

        // Ingest
        let result = service.ingest_url("https://test.com").await.unwrap();

        // Verify custom result returned
        assert_eq!(result.document_id, "custom-id");
        assert_eq!(result.title, "Custom Title");
        assert_eq!(result.word_count, 500);
        assert_eq!(result.chunks_created, 10);
    }

    #[tokio::test]
    async fn test_mock_web_ingestion_service_multiple_urls() {
        let service = MockWebIngestionService::new();

        // Ingest multiple URLs
        service.ingest_url("https://example.com/1").await.unwrap();
        service.ingest_url("https://example.com/2").await.unwrap();
        service.ingest_url("https://example.com/3").await.unwrap();

        // Verify all tracked in order
        let urls = service.get_ingested_urls();
        assert_eq!(urls.len(), 3);
        assert_eq!(urls[0], "https://example.com/1");
        assert_eq!(urls[1], "https://example.com/2");
        assert_eq!(urls[2], "https://example.com/3");
    }

    #[tokio::test]
    async fn test_mock_web_ingestion_service_clear() {
        let service = MockWebIngestionService::new();

        // Configure and ingest
        let result = WebIngestionResult {
            document_id: "test-id".to_string(),
            url: "https://test.com".to_string(),
            title: "Test".to_string(),
            word_count: 100,
            chunks_created: 5,
            site_name: None,
            author: None,
            reading_time_minutes: None,
        };
        service.set_result_for_url("https://test.com", result);
        service.ingest_url("https://test.com").await.unwrap();

        // Clear
        service.clear();

        // Verify cleared
        assert_eq!(service.get_ingested_urls().len(), 0);

        // Should return default result now
        let result = service.ingest_url("https://test.com").await.unwrap();
        assert_eq!(result.title, "Mock Web Document");
    }

    #[tokio::test]
    async fn test_mock_web_ingestion_service_deterministic() {
        let service = MockWebIngestionService::new();

        // Configure specific result
        let configured = WebIngestionResult {
            document_id: "fixed-id".to_string(),
            url: "https://example.com".to_string(),
            title: "Fixed Title".to_string(),
            word_count: 123,
            chunks_created: 7,
            site_name: None,
            author: None,
            reading_time_minutes: Some(1),
        };
        service.set_result_for_url("https://example.com", configured);

        // Multiple ingestions should return same result
        let result1 = service.ingest_url("https://example.com").await.unwrap();
        let result2 = service.ingest_url("https://example.com").await.unwrap();

        assert_eq!(result1.document_id, result2.document_id);
        assert_eq!(result1.title, result2.title);
        assert_eq!(result1.word_count, result2.word_count);
    }

    #[tokio::test]
    async fn test_mock_search_enrichment_service_default() {
        let service = MockSearchEnrichmentService::new();

        let chunk_ids = vec!["chunk1".to_string(), "chunk2".to_string()];
        let result = service.enrich_results(&chunk_ids).await.unwrap();

        // Should enrich all chunks with defaults
        assert_eq!(result.len(), 2);
        assert!(result.contains_key("chunk1"));
        assert!(result.contains_key("chunk2"));

        // Check default values
        let chunk1 = &result["chunk1"];
        assert_eq!(chunk1.snippet, "Mock content snippet for testing...");
        assert_eq!(
            chunk1.metadata["filename"],
            serde_json::Value::String("mock_document.txt".to_string())
        );
        assert_eq!(
            chunk1.metadata["file_type"],
            serde_json::Value::String("text/plain".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_search_enrichment_service_configured() {
        let service = MockSearchEnrichmentService::new();

        // Configure custom metadata
        let mut custom_metadata = std::collections::HashMap::new();
        custom_metadata.insert(
            "filename".to_string(),
            serde_json::Value::String("custom.txt".to_string()),
        );
        custom_metadata.insert(
            "file_type".to_string(),
            serde_json::Value::String("text/custom".to_string()),
        );

        let custom = crate::services::search_enrichment_service::DocumentMetadata {
            snippet: "Custom snippet text".to_string(),
            document_id: "chunk1".to_string(),
            metadata: custom_metadata,
        };
        service.set_metadata("chunk1", custom);

        // Enrich
        let result = service
            .enrich_results(&["chunk1".to_string()])
            .await
            .unwrap();

        // Verify custom metadata
        assert_eq!(result["chunk1"].snippet, "Custom snippet text");
        assert_eq!(
            result["chunk1"].metadata["filename"],
            serde_json::Value::String("custom.txt".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_search_enrichment_service_mixed() {
        let service = MockSearchEnrichmentService::new();

        // Configure one chunk, leave another as default
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "filename".to_string(),
            serde_json::Value::String("configured.txt".to_string()),
        );
        service.set_metadata(
            "chunk1",
            crate::services::search_enrichment_service::DocumentMetadata {
                snippet: "Configured".to_string(),
                document_id: "chunk1".to_string(),
                metadata,
            },
        );

        // Enrich both
        let chunk_ids = vec!["chunk1".to_string(), "chunk2".to_string()];
        let result = service.enrich_results(&chunk_ids).await.unwrap();

        // Verify mixed results
        assert_eq!(result["chunk1"].snippet, "Configured");
        assert_eq!(
            result["chunk2"].snippet,
            "Mock content snippet for testing..."
        );
    }

    #[tokio::test]
    async fn test_mock_search_enrichment_service_empty() {
        let service = MockSearchEnrichmentService::new();

        // Empty chunk IDs
        let result = service.enrich_results(&[]).await.unwrap();

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_mock_search_enrichment_service_clear() {
        let service = MockSearchEnrichmentService::new();

        // Configure metadata
        service.set_metadata(
            "chunk1",
            crate::services::search_enrichment_service::DocumentMetadata {
                snippet: "Test".to_string(),
                document_id: "chunk1".to_string(),
                metadata: std::collections::HashMap::new(),
            },
        );

        // Clear
        service.clear();

        // Should return default now
        let result = service
            .enrich_results(&["chunk1".to_string()])
            .await
            .unwrap();
        assert_eq!(
            result["chunk1"].snippet,
            "Mock content snippet for testing..."
        );
    }

    #[tokio::test]
    async fn test_mock_search_enrichment_service_batch() {
        let service = MockSearchEnrichmentService::new();

        // Large batch
        let chunk_ids: Vec<String> = (0..100).map(|i| format!("chunk{}", i)).collect();
        let result = service.enrich_results(&chunk_ids).await.unwrap();

        // All chunks enriched
        assert_eq!(result.len(), 100);
        for i in 0..100 {
            assert!(result.contains_key(&format!("chunk{}", i)));
        }
    }

    #[tokio::test]
    async fn test_mock_conversation_service_create() {
        let service = MockConversationService::new();

        let conversation = service
            .create_conversation(
                "Test Chat".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                Some("You are helpful.".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(conversation.title, "Test Chat");
        assert_eq!(conversation.model_name, "claude-sonnet-4-5-20250929");
        assert_eq!(
            conversation.system_prompt,
            Some("You are helpful.".to_string())
        );
        assert_eq!(conversation.message_count, 0);
        assert_eq!(conversation.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_mock_conversation_service_messages() {
        let service = MockConversationService::new();

        let conversation = service
            .create_conversation("Test".to_string(), "model".to_string(), None)
            .await
            .unwrap();

        // Add user message
        let msg1 = service
            .add_user_message(&conversation.id.to_string(), "Hello".to_string(), 10)
            .await
            .unwrap();
        assert_eq!(msg1.content, "Hello");
        assert_eq!(msg1.tokens, 10);

        // Add assistant message
        let msg2 = service
            .add_assistant_message(&conversation.id.to_string(), "Hi there".to_string(), 20)
            .await
            .unwrap();
        assert_eq!(msg2.content, "Hi there");
        assert_eq!(msg2.tokens, 20);

        // Verify aggregate
        let aggregate = service
            .get_conversation(&conversation.id.to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(aggregate.messages().len(), 2);
        assert_eq!(aggregate.total_tokens(), 30);
    }

    #[tokio::test]
    async fn test_mock_conversation_service_list() {
        let service = MockConversationService::new();

        // Create multiple conversations
        service
            .create_conversation("Conv 1".to_string(), "model".to_string(), None)
            .await
            .unwrap();
        service
            .create_conversation("Conv 2".to_string(), "model".to_string(), None)
            .await
            .unwrap();
        service
            .create_conversation("Conv 3".to_string(), "model".to_string(), None)
            .await
            .unwrap();

        // List all
        let conversations = service.list_conversations(None, None).await.unwrap();
        assert_eq!(conversations.len(), 3);

        // List with limit
        let limited = service.list_conversations(Some(2), None).await.unwrap();
        assert_eq!(limited.len(), 2);

        // List with offset
        let offset = service.list_conversations(Some(10), Some(1)).await.unwrap();
        assert_eq!(offset.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_conversation_service_prune() {
        let service = MockConversationService::new();

        let conversation = service
            .create_conversation("Test".to_string(), "model".to_string(), None)
            .await
            .unwrap();

        // Add multiple messages
        for i in 0..5 {
            service
                .add_user_message(&conversation.id.to_string(), format!("Message {}", i), 100)
                .await
                .unwrap();
        }

        // Prune to 250 tokens (should keep last 2-3 messages)
        service
            .prune_conversation_to_limit(&conversation.id.to_string(), 250)
            .await
            .unwrap();

        let aggregate = service
            .get_conversation(&conversation.id.to_string())
            .await
            .unwrap()
            .unwrap();

        assert!(aggregate.total_tokens() <= 250);
        assert!(!aggregate.messages().is_empty()); // At least last message
    }
}

// ============================================================================
// Document Repository Trait (Legacy)
// ============================================================================
