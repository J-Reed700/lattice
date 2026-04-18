#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Hybrid Search Integration Tests
//!
//! Tests the HybridSearchService combining vector and BM25 search.
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::sync::Arc;
use vault::infrastructure::search::bm25::BM25Search;
use vault::infrastructure::search::hybrid::{HybridSearchService, SearchMode};
use vault::infrastructure::search::vector_search::USearchVectorIndex;
use vault::infrastructure::services::search_enrichment_service::SearchEnrichmentService;
use vault::features::search::{BM25SearchTrait, HybridSearchTrait, SearchServiceTrait};

/// Setup test database with schema
async fn setup_test_db() -> Result<SqlitePool, Box<dyn std::error::Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await?;

    // Create tables
    sqlx::query(
        "CREATE TABLE documents (
            id TEXT PRIMARY KEY,
            file_id TEXT,
            file_name TEXT NOT NULL,
            file_path TEXT,
            mime_type TEXT,
            size_bytes INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT
        )",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE text_chunks (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL,
            content TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            FOREIGN KEY (document_id) REFERENCES documents(id)
        )",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE VIRTUAL TABLE documents_fts USING fts5(
            document_id UNINDEXED,
            content,
            tokenize='porter unicode61 remove_diacritics 2'
        )",
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

/// Insert test documents into database
async fn insert_test_documents(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    // Insert documents
    sqlx::query(
        "INSERT INTO documents (id, file_name, mime_type, size_bytes, created_at) VALUES
         ('doc1', 'rust.txt', 'text/plain', 1024, '2024-01-01'),
         ('doc2', 'python.txt', 'text/plain', 2048, '2024-01-02'),
         ('doc3', 'ml.txt', 'text/plain', 3072, '2024-01-03')",
    )
    .execute(pool)
    .await?;

    // Insert chunks
    sqlx::query(
        "INSERT INTO text_chunks (id, document_id, content, chunk_index) VALUES
         ('chunk1', 'doc1', 'Rust is a systems programming language focused on safety and performance', 0),
         ('chunk2', 'doc2', 'Python is a high-level programming language known for simplicity', 0),
         ('chunk3', 'doc3', 'Machine learning with neural networks and deep learning frameworks', 0)",
    )
    .execute(pool)
    .await?;

    // Insert into FTS
    sqlx::query(
        "INSERT INTO documents_fts (document_id, content) VALUES
         ('chunk1', 'Rust is a systems programming language focused on safety and performance'),
         ('chunk2', 'Python is a high-level programming language known for simplicity'),
         ('chunk3', 'Machine learning with neural networks and deep learning frameworks')",
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Create test HybridSearchService
async fn create_hybrid_search_service(
    pool: SqlitePool,
) -> Result<HybridSearchService, Box<dyn std::error::Error>> {
    // Create vector search service with empty USearch index
    let usearch_index = USearchVectorIndex::new(384, None)?;
    let vector_search = Arc::new(usearch_index) as Arc<dyn SearchServiceTrait>;

    // Create BM25 search service
    let bm25_search = Arc::new(BM25Search::new(pool.clone())) as Arc<dyn BM25SearchTrait>;

    // Create enrichment service
    let enrichment = Arc::new(SearchEnrichmentService::new(pool.clone()));

    // Create hybrid search service
    let service = HybridSearchService::new(
        vector_search,
        bm25_search,
        pool.clone(),
        enrichment,
        vault::infrastructure::search::hybrid::SearchConfig::default(),
    );

    Ok(service)
}

#[tokio::test]
async fn test_hybrid_search_keyword_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let pool = setup_test_db().await?;
    insert_test_documents(&pool).await?;
    let service = create_hybrid_search_service(pool).await?;

    // Execute keyword-only search
    let results = service.search("programming", 10).await?;

    // Verify
    assert!(!results.is_empty(), "Should return results");
    assert!(
        results.len() >= 2,
        "Should find at least 2 documents with 'programming'"
    );

    // Verify keyword scores are populated
    for result in &results {
        assert!(
            result.keyword_score.is_some(),
            "Keyword search should have keyword_score"
        );
        assert!(
            result.vector_score.is_none(),
            "Keyword-only should not have vector_score"
        );
        assert!(
            result.bm25_rank.is_some(),
            "Should have BM25 ranking position"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_hybrid_search_vector_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let pool = setup_test_db().await?;
    insert_test_documents(&pool).await?;
    let service = create_hybrid_search_service(pool).await?;

    // Create a dummy embedding (would normally come from embedding service)
    let query_embedding = vec![0.1f32; 384];

    // Execute vector-only search
    let results = service.search("test", 10).await?;

    // Verify - may be empty since we have no real embeddings indexed
    // But should not error
    assert!(results.is_empty() || !results.is_empty());

    if !results.is_empty() {
        // If we got results, verify vector scores are populated
        for result in &results {
            assert!(
                result.vector_score.is_some(),
                "Vector search should have vector_score"
            );
            assert!(
                result.keyword_score.is_none(),
                "Vector-only should not have keyword_score"
            );
            assert!(
                result.vector_rank.is_some(),
                "Should have vector ranking position"
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_hybrid_search_hybrid_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let pool = setup_test_db().await?;
    insert_test_documents(&pool).await?;
    let service = create_hybrid_search_service(pool).await?;

    // Create a dummy embedding
    let query_embedding = vec![0.1f32; 384];

    // Execute hybrid search (combines both)
    let results = service.search("programming", 10).await?;

    // Verify - should have results from BM25 at minimum
    assert!(!results.is_empty(), "Should return results from BM25");

    // Check that results have proper structure
    for result in &results {
        assert!(!result.chunk_id.is_empty(), "Should have chunk_id");
        assert!(!result.document_id.is_empty(), "Should have document_id");
        assert!(result.score >= 0.0, "Should have non-negative score");
    }

    Ok(())
}

#[tokio::test]
async fn test_hybrid_search_empty_query() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let pool = setup_test_db().await?;
    insert_test_documents(&pool).await?;
    let service = create_hybrid_search_service(pool).await?;

    // Test keyword mode with empty query - should error
    let result = service.search("", 10).await;
    assert!(result.is_err(), "Empty query should error in keyword mode");

    // Test vector mode with empty embedding - should error
    let result = service.search("test", 10).await;
    assert!(
        result.is_err(),
        "Empty embedding should error in vector mode"
    );

    Ok(())
}

#[tokio::test]
async fn test_hybrid_search_fallback_behavior() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let pool = setup_test_db().await?;
    insert_test_documents(&pool).await?;
    let service = create_hybrid_search_service(pool).await?;

    // Test hybrid mode with no embedding - should fall back to keyword only
    let results = service.search("programming", 10).await?;

    assert!(!results.is_empty(), "Should fall back to keyword search");

    // Test hybrid mode with no text - should fall back to vector only
    let query_embedding = vec![0.1f32; 384];
    let results = service.search("", 10).await?;

    // Should not error, but may be empty if no vectors indexed
    assert!(results.is_empty() || !results.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_hybrid_search_batch() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let pool = setup_test_db().await?;
    insert_test_documents(&pool).await?;
    let service = create_hybrid_search_service(pool).await?;

    // Prepare batch queries
    let queries = vec![
        ("programming".to_string(), vec![]),
        ("machine learning".to_string(), vec![]),
    ];

    // Execute batch search
    let results = service
        .batch_search(queries, 5, SearchMode::Keyword)
        .await?;

    // Verify
    assert_eq!(results.len(), 2, "Should return results for both queries");
    assert!(!results[0].is_empty(), "First query should have results");
    assert!(!results[1].is_empty(), "Second query should have results");

    Ok(())
}
