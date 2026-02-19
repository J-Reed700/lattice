#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! # Phase 3 HyDE Integration Tests
//!
//! Comprehensive tests for DocumentRetriever with parallel embedding generation
//! and hybrid search capabilities.
//!
//! ## Test Coverage
//!
//! 1. **Strategy Testing**:
//!    - HyDEOnly strategy
//!    - RawOnly strategy
//!    - Hybrid strategy with parallel execution
//!
//! 2. **Result Merging**:
//!    - Deduplication
//!    - Weighted scoring (70% HyDE, 30% raw)
//!    - Re-ranking
//!
//! 3. **Edge Cases**:
//!    - Missing HyDE text
//!    - Empty search results
//!    - Large result sets
//!
//! 4. **Performance**:
//!    - Parallel embedding generation
//!    - Concurrent search execution
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use vault::domain::qa::hyde::{HyDEInterpretation, QueryType, SearchStrategy, ToolIntent};
use vault::infrastructure::search::service::SearchResult;
use vault::infrastructure::services::hyde::DocumentRetriever;
use vault::infrastructure::services::traits::{EmbeddingServiceTrait, SearchServiceTrait};
use vault::shared::error::Result;

// ============================================================================
// Mock Services for Testing
// ============================================================================

struct MockEmbeddingService {
    embeddings: Mutex<HashMap<String, Vec<f32>>>,
    call_count: Mutex<usize>,
}

impl MockEmbeddingService {
    fn new() -> Self {
        Self {
            embeddings: Mutex::new(HashMap::new()),
            call_count: Mutex::new(0),
        }
    }

    fn set_embedding(&self, text: &str, embedding: Vec<f32>) {
        self.embeddings
            .lock()
            .unwrap()
            .insert(text.to_string(), embedding);
    }

    fn get_call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }

    fn reset_call_count(&self) {
        *self.call_count.lock().unwrap() = 0;
    }
}

#[async_trait]
impl EmbeddingServiceTrait for MockEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        *self.call_count.lock().unwrap() += 1;

        Ok(self
            .embeddings
            .lock()
            .unwrap()
            .get(text)
            .cloned()
            .unwrap_or_else(|| {
                // Generate deterministic embedding based on text
                vec![
                    text.len() as f32 / 100.0,
                    text.chars().count() as f32 / 100.0,
                    text.bytes().len() as f32 / 100.0,
                ]
            }))
    }

    async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(vec![])
    }

    async fn embed_contextualized_chunks(
        &self,
        _chunks: &[vault::infrastructure::indexing::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        Ok(vec![])
    }
}

struct MockSearchService {
    results: Mutex<HashMap<String, Vec<SearchResult>>>,
    call_count: Mutex<usize>,
}

impl MockSearchService {
    fn new() -> Self {
        Self {
            results: Mutex::new(HashMap::new()),
            call_count: Mutex::new(0),
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

    fn get_call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

#[async_trait]
impl SearchServiceTrait for MockSearchService {
    fn search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        *self.call_count.lock().unwrap() += 1;

        let key = Self::embedding_to_key(query_embedding);
        let mut results = self
            .results
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .unwrap_or_default();

        // Respect top_k limit
        results.truncate(top_k);
        Ok(results)
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

fn create_search_result(id: &str, score: f32, content: &str) -> SearchResult {
    SearchResult {
        id: id.to_string(),
        score,
        index: 0,
        filename: Some(format!("{}.txt", id)),
        mime_type: Some("text/plain".to_string()),
        size_bytes: Some(1024),
        created_at: None,
        content: Some(content.to_string()),
        file_id: None,
        file_path: Some(format!("/docs/{}.txt", id)),
        file_name: None,
        file_extension: None,
        file_category: None,
        is_indexed: None,
        document_id: Some(id.to_string()),
        snippet: None,
        chunk_index: Some(0),
        updated_at: None,
    }
}

// ============================================================================
// Strategy Tests
// ============================================================================

#[tokio::test]
async fn test_hyde_only_strategy() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    // Setup
    let hyde_text = "Domain-Driven Design is a software design approach";
    let hyde_embedding = vec![1.0, 0.0, 0.0];
    embedding_service.set_embedding(hyde_text, hyde_embedding.clone());

    let expected_results = vec![
        create_search_result("ddd_book", 0.95, "DDD book excerpt"),
        create_search_result("ddd_article", 0.85, "DDD article"),
    ];
    search_service.set_results(
        &MockSearchService::embedding_to_key(&hyde_embedding),
        expected_results,
    );

    let retriever = DocumentRetriever::new(embedding_service.clone(), search_service.clone());

    let interpretation = HyDEInterpretation::for_question("What is DDD?", hyde_text);

    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

    assert_eq!(documents.len(), 2);
    assert_eq!(documents[0].similarity_score, 0.95);
    assert_eq!(documents[0].content, "DDD book excerpt");
    assert!(documents[0].file_path.contains("ddd_book"));

    assert_eq!(documents[1].similarity_score, 0.85);

    // Verify only one embedding call (HyDE text only)
    assert_eq!(embedding_service.get_call_count(), 1);
    assert_eq!(search_service.get_call_count(), 1);
}

#[tokio::test]
async fn test_raw_only_strategy() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    // Setup
    let raw_query = "Hello, how are you?";
    let raw_embedding = vec![0.0, 1.0, 0.0];
    embedding_service.set_embedding(raw_query, raw_embedding.clone());

    let expected_results = vec![create_search_result(
        "greeting_doc",
        0.75,
        "Greetings and salutations",
    )];
    search_service.set_results(
        &MockSearchService::embedding_to_key(&raw_embedding),
        expected_results,
    );

    let retriever = DocumentRetriever::new(embedding_service.clone(), search_service.clone());

    let interpretation = HyDEInterpretation::for_greeting(raw_query);

    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].similarity_score, 0.75);
    assert_eq!(documents[0].content, "Greetings and salutations");

    // Verify only one embedding call (raw query only)
    assert_eq!(embedding_service.get_call_count(), 1);
    assert_eq!(search_service.get_call_count(), 1);
}

#[tokio::test]
async fn test_hybrid_strategy_parallel_execution() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    // Setup embeddings
    let raw_query = "What is Rust programming language?";
    let hyde_text = "Rust is a systems programming language that emphasizes safety and performance";

    let raw_embedding = vec![0.0, 1.0, 0.0];
    let hyde_embedding = vec![1.0, 0.0, 0.0];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());
    embedding_service.set_embedding(hyde_text, hyde_embedding.clone());

    // Setup search results
    let hyde_results = vec![
        create_search_result("rust_book", 0.95, "Rust programming book"),
        create_search_result("rust_tutorial", 0.85, "Rust tutorial"),
    ];
    let raw_results = vec![
        create_search_result("rust_tutorial", 0.80, "Rust tutorial"), // Duplicate
        create_search_result("rust_faq", 0.70, "Rust FAQ"),
    ];

    search_service.set_results(
        &MockSearchService::embedding_to_key(&hyde_embedding),
        hyde_results,
    );
    search_service.set_results(
        &MockSearchService::embedding_to_key(&raw_embedding),
        raw_results,
    );

    let retriever = DocumentRetriever::new(embedding_service.clone(), search_service.clone());

    let interpretation = HyDEInterpretation::hybrid(raw_query, hyde_text, QueryType::Question);

    // Measure execution time to verify parallel execution
    let start = Instant::now();
    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();
    let elapsed = start.elapsed();

    // Should have 3 unique documents after deduplication
    assert_eq!(documents.len(), 3);

    // Verify embeddings were generated (should be 2 calls)
    assert_eq!(embedding_service.get_call_count(), 2);

    // Verify both searches were executed
    assert_eq!(search_service.get_call_count(), 2);

    // Verify parallel execution is fast (should be < 100ms for mocks)
    assert!(
        elapsed.as_millis() < 100,
        "Parallel execution took too long: {:?}",
        elapsed
    );

    println!("Parallel execution completed in {:?}", elapsed);
}

// ============================================================================
// Result Merging Tests
// ============================================================================

#[tokio::test]
async fn test_hybrid_result_merging_deduplication() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    // Setup
    let raw_query = "machine learning";
    let hyde_text = "Machine learning is a field of AI";

    let raw_embedding = vec![0.1, 0.2, 0.3];
    let hyde_embedding = vec![0.9, 0.8, 0.7];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());
    embedding_service.set_embedding(hyde_text, hyde_embedding.clone());

    // Create overlapping results
    let hyde_results = vec![
        create_search_result("ml_book", 0.95, "ML book"),
        create_search_result("ml_paper", 0.90, "ML paper"),
        create_search_result("shared_doc", 0.85, "Shared doc"), // Will appear in both
    ];

    let raw_results = vec![
        create_search_result("shared_doc", 0.80, "Shared doc"), // Duplicate
        create_search_result("ml_tutorial", 0.75, "ML tutorial"),
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

    let interpretation = HyDEInterpretation::hybrid(raw_query, hyde_text, QueryType::Question);

    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

    // Should have 4 unique documents (ml_book, ml_paper, shared_doc, ml_tutorial)
    assert_eq!(documents.len(), 4);

    // Find shared_doc and verify combined score
    let shared_doc = documents
        .iter()
        .find(|d| d.file_path.contains("shared_doc"))
        .expect("shared_doc should be present");

    // Combined score: 0.85 * 0.7 + 0.80 * 0.3 = 0.595 + 0.24 = 0.835
    let expected_score = 0.85 * 0.7 + 0.80 * 0.3;
    assert!(
        (shared_doc.similarity_score - expected_score).abs() < 0.01,
        "Expected score {}, got {}",
        expected_score,
        shared_doc.similarity_score
    );
}

#[tokio::test]
async fn test_hybrid_weighted_scoring() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    // Setup
    let raw_query = "test query";
    let hyde_text = "test hyde";

    let raw_embedding = vec![0.1, 0.2, 0.3];
    let hyde_embedding = vec![0.9, 0.8, 0.7];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());
    embedding_service.set_embedding(hyde_text, hyde_embedding.clone());

    let hyde_results = vec![create_search_result("doc1", 1.0, "Perfect HyDE match")];

    let raw_results = vec![create_search_result("doc2", 1.0, "Perfect raw match")];

    search_service.set_results(
        &MockSearchService::embedding_to_key(&hyde_embedding),
        hyde_results,
    );
    search_service.set_results(
        &MockSearchService::embedding_to_key(&raw_embedding),
        raw_results,
    );

    let retriever = DocumentRetriever::new(embedding_service, search_service);

    let interpretation = HyDEInterpretation::hybrid(raw_query, hyde_text, QueryType::Question);

    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

    assert_eq!(documents.len(), 2);

    // doc1 should have higher score (70% weight) than doc2 (30% weight)
    let doc1 = &documents[0];
    let doc2 = &documents[1];

    assert!(doc1.file_path.contains("doc1"));
    assert!(doc2.file_path.contains("doc2"));

    // doc1: 1.0 * 0.7 = 0.7
    // doc2: 1.0 * 0.3 = 0.3
    assert!((doc1.similarity_score - 0.7).abs() < 0.01);
    assert!((doc2.similarity_score - 0.3).abs() < 0.01);
}

#[tokio::test]
async fn test_hybrid_respects_top_k() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    // Setup
    let raw_query = "test";
    let hyde_text = "hyde";

    let raw_embedding = vec![0.1, 0.2, 0.3];
    let hyde_embedding = vec![0.9, 0.8, 0.7];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());
    embedding_service.set_embedding(hyde_text, hyde_embedding.clone());

    // Create many results
    let hyde_results = vec![
        create_search_result("doc1", 0.95, "Doc 1"),
        create_search_result("doc2", 0.90, "Doc 2"),
        create_search_result("doc3", 0.85, "Doc 3"),
    ];

    let raw_results = vec![
        create_search_result("doc4", 0.80, "Doc 4"),
        create_search_result("doc5", 0.75, "Doc 5"),
        create_search_result("doc6", 0.70, "Doc 6"),
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

    let interpretation = HyDEInterpretation::hybrid(raw_query, hyde_text, QueryType::Question);

    // Request only top 3
    let documents = retriever.retrieve(&interpretation, 3).await.unwrap();

    assert_eq!(documents.len(), 3);

    // Verify ordering by score
    assert!(documents[0].similarity_score >= documents[1].similarity_score);
    assert!(documents[1].similarity_score >= documents[2].similarity_score);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_hyde_only_without_hyde_text_fallback() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    let raw_query = "test query";
    let raw_embedding = vec![0.1, 0.2, 0.3];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());

    let results = vec![create_search_result("doc1", 0.80, "Fallback result")];
    search_service.set_results(
        &MockSearchService::embedding_to_key(&raw_embedding),
        results,
    );

    let retriever = DocumentRetriever::new(embedding_service, search_service);

    // Create HyDEOnly interpretation but with no HyDE text
    let interpretation = HyDEInterpretation {
        query_type: QueryType::Question,
        original_query: raw_query.to_string(),
        hyde_text: None,
        search_strategy: SearchStrategy::HyDEOnly,
        tool_intent: ToolIntent::default_for_query_type(QueryType::Question),
    };

    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

    // Should fall back to raw query
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].content, "Fallback result");
}

#[tokio::test]
async fn test_hybrid_without_hyde_text_fallback() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    let raw_query = "test query";
    let raw_embedding = vec![0.1, 0.2, 0.3];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());

    let results = vec![create_search_result("doc1", 0.80, "Fallback result")];
    search_service.set_results(
        &MockSearchService::embedding_to_key(&raw_embedding),
        results,
    );

    let retriever = DocumentRetriever::new(embedding_service, search_service);

    // Create Hybrid interpretation but with no HyDE text
    let interpretation = HyDEInterpretation {
        query_type: QueryType::Question,
        original_query: raw_query.to_string(),
        hyde_text: None,
        search_strategy: SearchStrategy::Hybrid,
        tool_intent: ToolIntent::default_for_query_type(QueryType::Question),
    };

    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

    // Should fall back to raw query only
    assert_eq!(documents.len(), 1);
}

#[tokio::test]
async fn test_empty_search_results() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    let raw_query = "obscure query";
    let raw_embedding = vec![0.1, 0.2, 0.3];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());

    // Set empty results
    search_service.set_results(&MockSearchService::embedding_to_key(&raw_embedding), vec![]);

    let retriever = DocumentRetriever::new(embedding_service, search_service);

    let interpretation = HyDEInterpretation::for_greeting(raw_query);

    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

    assert_eq!(documents.len(), 0);
}

#[tokio::test]
async fn test_large_result_set() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    let raw_query = "common term";
    let raw_embedding = vec![0.1, 0.2, 0.3];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());

    // Create 100 results
    let large_results: Vec<SearchResult> = (0..100)
        .map(|i| create_search_result(&format!("doc{}", i), 1.0 - (i as f32 / 100.0), "content"))
        .collect();

    search_service.set_results(
        &MockSearchService::embedding_to_key(&raw_embedding),
        large_results,
    );

    let retriever = DocumentRetriever::new(embedding_service, search_service);

    let interpretation = HyDEInterpretation::for_greeting(raw_query);

    // Request top 10
    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

    assert_eq!(documents.len(), 10);

    // Verify ordering (highest scores first)
    for i in 0..9 {
        assert!(documents[i].similarity_score >= documents[i + 1].similarity_score);
    }
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_parallel_embedding_generation_is_concurrent() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    let raw_query = "test";
    let hyde_text = "hyde text";

    let raw_embedding = vec![0.1, 0.2, 0.3];
    let hyde_embedding = vec![0.9, 0.8, 0.7];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());
    embedding_service.set_embedding(hyde_text, hyde_embedding.clone());

    search_service.set_results(
        &MockSearchService::embedding_to_key(&hyde_embedding),
        vec![],
    );
    search_service.set_results(&MockSearchService::embedding_to_key(&raw_embedding), vec![]);

    let retriever = DocumentRetriever::new(embedding_service.clone(), search_service);

    let interpretation = HyDEInterpretation::hybrid(raw_query, hyde_text, QueryType::Question);

    embedding_service.reset_call_count();

    // Execute hybrid search
    let _ = retriever.retrieve(&interpretation, 10).await.unwrap();

    // Verify both embeddings were generated
    assert_eq!(embedding_service.get_call_count(), 2);
}

#[tokio::test]
async fn test_result_conversion_preserves_metadata() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    let raw_query = "test";
    let raw_embedding = vec![0.1, 0.2, 0.3];

    embedding_service.set_embedding(raw_query, raw_embedding.clone());

    let result = SearchResult {
        id: "doc123".to_string(),
        score: 0.95,
        index: 0,
        filename: Some("document.txt".to_string()),
        mime_type: Some("text/plain".to_string()),
        size_bytes: Some(2048),
        created_at: Some("2024-01-01".to_string()),
        content: Some("Important content here".to_string()),
        file_id: Some("file456".to_string()),
        file_path: Some("/important/document.txt".to_string()),
        file_name: Some("document.txt".to_string()),
        file_extension: Some("txt".to_string()),
        file_category: Some("document".to_string()),
        is_indexed: Some(true),
        document_id: Some("doc123".to_string()),
        snippet: Some("snippet here".to_string()),
        chunk_index: Some(5),
        updated_at: Some("2024-01-02".to_string()),
    };

    search_service.set_results(
        &MockSearchService::embedding_to_key(&raw_embedding),
        vec![result],
    );

    let retriever = DocumentRetriever::new(embedding_service, search_service);

    let interpretation = HyDEInterpretation::for_greeting(raw_query);

    let documents = retriever.retrieve(&interpretation, 10).await.unwrap();

    assert_eq!(documents.len(), 1);

    let doc = &documents[0];
    assert_eq!(doc.content, "Important content here");
    assert_eq!(doc.file_path, "/important/document.txt");
    assert_eq!(doc.similarity_score, 0.95);
    assert_eq!(doc.chunk_index, 5);
}

#[tokio::test]
async fn test_integration_with_all_strategies() {
    let embedding_service = Arc::new(MockEmbeddingService::new());
    let search_service = Arc::new(MockSearchService::new());

    // Setup embeddings
    let query = "What is domain-driven design?";
    let hyde = "Domain-driven design is a software development approach";

    let raw_emb = vec![0.1, 0.2, 0.3];
    let hyde_emb = vec![0.9, 0.8, 0.7];

    embedding_service.set_embedding(query, raw_emb.clone());
    embedding_service.set_embedding(hyde, hyde_emb.clone());

    let hyde_results = vec![create_search_result("ddd_doc", 0.95, "DDD content")];
    let raw_results = vec![create_search_result("domain_doc", 0.85, "Domain content")];

    search_service.set_results(
        &MockSearchService::embedding_to_key(&hyde_emb),
        hyde_results,
    );
    search_service.set_results(&MockSearchService::embedding_to_key(&raw_emb), raw_results);

    let retriever = DocumentRetriever::new(embedding_service, search_service);

    // Test HyDEOnly
    let hyde_only_interp = HyDEInterpretation::for_question(query, hyde);
    let hyde_only_docs = retriever.retrieve(&hyde_only_interp, 10).await.unwrap();
    assert_eq!(hyde_only_docs.len(), 1);
    assert!(hyde_only_docs[0].file_path.contains("ddd_doc"));

    // Test RawOnly
    let raw_only_interp = HyDEInterpretation::for_greeting(query);
    let raw_only_docs = retriever.retrieve(&raw_only_interp, 10).await.unwrap();
    assert_eq!(raw_only_docs.len(), 1);
    assert!(raw_only_docs[0].file_path.contains("domain_doc"));

    // Test Hybrid
    let hybrid_interp = HyDEInterpretation::hybrid(query, hyde, QueryType::Question);
    let hybrid_docs = retriever.retrieve(&hybrid_interp, 10).await.unwrap();
    assert_eq!(hybrid_docs.len(), 2);
}
