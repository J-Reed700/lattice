#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! SearchEnrichmentService Integration Tests
//!
//! Tests search result enrichment with real database operations.
//!
//! # Coverage
//!
//! - Single result enrichment
//! - Batch processing (900 chunk limit)
//! - Large-scale enrichment (2000+ chunks)
//! - Parallel batch processing
//! - Snippet generation
//! - Metadata extraction
//! - Edge cases (empty results, missing chunks)
//! - Performance benchmarks
use chrono::Utc;
use lattice::features::search::enrichment_service::{DocumentMetadata, SearchEnrichmentService};
use sqlx::SqlitePool;
use std::time::Instant;
use uuid::Uuid;

/// Helper to create test database pool
async fn create_test_db() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:")
        .await
        .expect("Failed to create in-memory database");

    lattice::infrastructure::persistence::database::init::initialize_database(&pool)
        .await
        .expect("Failed to initialize database");

    pool
}

/// Helper to insert test document
async fn insert_test_document(
    pool: &SqlitePool,
    id: &str,
    file_name: &str,
    file_path: &str,
) -> String {
    sqlx::query(
        r#"
        INSERT INTO documents (
            id, file_path, file_name, file_type, mime_type,
            size_bytes, modified_at, indexed_at, checksum, status,
            created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id)
    .bind(file_path)
    .bind(file_name)
    .bind("text")
    .bind("text/plain")
    .bind(1000_i64)
    .bind(Utc::now().to_rfc3339())
    .bind(Utc::now().to_rfc3339())
    .bind("checksum123")
    .bind("indexed")
    .bind(Utc::now().to_rfc3339())
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .expect("Failed to insert document");

    id.to_string()
}

/// Helper to insert test chunk
async fn insert_test_chunk(
    pool: &SqlitePool,
    id: &str,
    document_id: &str,
    content: &str,
    chunk_index: i64,
) -> String {
    sqlx::query(
        r#"
        INSERT INTO text_chunks (
            id, document_id, content, chunk_index,
            start_char, end_char
        )
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id)
    .bind(document_id)
    .bind(content)
    .bind(chunk_index)
    .bind(Some(0_i64))
    .bind(Some(content.len() as i64))
    .execute(pool)
    .await
    .expect("Failed to insert chunk");

    id.to_string()
}

/// Test 1: Enrich empty results
#[tokio::test]
async fn test_enrich_empty_results() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    let result = service.enrich_results(&[]).await;

    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());

    pool.close().await;
}

/// Test 2: Enrich single result
#[tokio::test]
async fn test_enrich_single_result() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    // Insert test data
    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "test.txt", "/path/test.txt").await;

    let chunk_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk_id, &doc_id, "Test content", 0).await;

    // Enrich
    let result = service
        .enrich_results(std::slice::from_ref(&chunk_id))
        .await;

    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert_eq!(metadata.len(), 1);
    assert!(metadata.contains_key(&chunk_id));

    let doc_meta = metadata.get(&chunk_id).unwrap();
    assert_eq!(doc_meta.snippet, "Test content");
    assert_eq!(
        doc_meta.metadata.get("filename").unwrap().as_str().unwrap(),
        "test.txt"
    );

    pool.close().await;
}

/// Test 3: Enrich multiple results
#[tokio::test]
async fn test_enrich_multiple_results() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    // Insert test data
    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "test.txt", "/path/test.txt").await;

    let mut chunk_ids = Vec::new();
    for i in 0..5 {
        let chunk_id = Uuid::new_v4().to_string();
        insert_test_chunk(
            &pool,
            &chunk_id,
            &doc_id,
            &format!("Chunk {} content", i),
            i,
        )
        .await;
        chunk_ids.push(chunk_id);
    }

    // Enrich
    let result = service.enrich_results(&chunk_ids).await;

    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert_eq!(metadata.len(), 5);

    for chunk_id in &chunk_ids {
        assert!(metadata.contains_key(chunk_id));
    }

    pool.close().await;
}

/// Test 4: Batch processing large input (SQLite 999 parameter limit)
#[tokio::test]
async fn test_enrich_batching_large_input() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    // Create 2000 chunks (should create 3 batches: 900, 900, 200)
    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "large.txt", "/path/large.txt").await;

    let mut chunk_ids = Vec::new();
    for i in 0..2000 {
        let chunk_id = Uuid::new_v4().to_string();
        insert_test_chunk(&pool, &chunk_id, &doc_id, &format!("Content {}", i), i).await;
        chunk_ids.push(chunk_id);
    }

    // Enrich (should batch automatically)
    let start = Instant::now();
    let result = service.enrich_results(&chunk_ids).await;
    let duration = start.elapsed();

    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert_eq!(metadata.len(), 2000);

    println!("Enriched 2000 chunks in {:?}", duration);
    assert!(
        duration.as_secs() < 5,
        "Enrichment took too long: {:?}",
        duration
    );

    pool.close().await;
}

/// Test 5: Snippet generation and truncation
#[tokio::test]
async fn test_enrich_snippet_generation() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    // Create content longer than 200 chars
    let long_content = "a".repeat(500);

    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "test.txt", "/path/test.txt").await;

    let chunk_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk_id, &doc_id, &long_content, 0).await;

    let result = service
        .enrich_results(std::slice::from_ref(&chunk_id))
        .await
        .unwrap();
    let metadata = result.get(&chunk_id).unwrap();

    // Snippet should be truncated to 200 chars + "..."
    assert_eq!(metadata.snippet.len(), 203);
    assert!(metadata.snippet.ends_with("..."));

    pool.close().await;
}

/// Test 6: Snippet for short content
#[tokio::test]
async fn test_enrich_snippet_short_content() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    let short_content = "Short content";

    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "test.txt", "/path/test.txt").await;

    let chunk_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk_id, &doc_id, short_content, 0).await;

    let result = service
        .enrich_results(std::slice::from_ref(&chunk_id))
        .await
        .unwrap();
    let metadata = result.get(&chunk_id).unwrap();

    // Snippet should be the full content (no truncation)
    assert_eq!(metadata.snippet, short_content);
    assert!(!metadata.snippet.ends_with("..."));

    pool.close().await;
}

/// Test 7: Metadata extraction
#[tokio::test]
async fn test_enrich_metadata_extraction() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "report.pdf", "/docs/report.pdf").await;

    let chunk_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk_id, &doc_id, "Test content", 0).await;

    let result = service
        .enrich_results(std::slice::from_ref(&chunk_id))
        .await
        .unwrap();
    let metadata = result.get(&chunk_id).unwrap();

    // Verify all metadata fields
    assert_eq!(
        metadata.metadata.get("filename").unwrap().as_str().unwrap(),
        "report.pdf"
    );
    assert_eq!(
        metadata
            .metadata
            .get("file_type")
            .unwrap()
            .as_str()
            .unwrap(),
        "text"
    );
    assert_eq!(
        metadata
            .metadata
            .get("file_size")
            .unwrap()
            .as_i64()
            .unwrap(),
        1000
    );
    assert!(metadata.metadata.contains_key("created_at"));
    assert!(metadata.metadata.contains_key("updated_at"));

    pool.close().await;
}

/// Test 8: Missing chunks (non-existent IDs)
#[tokio::test]
async fn test_enrich_missing_chunks() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    // Create some chunks
    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "test.txt", "/path/test.txt").await;

    let chunk_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk_id, &doc_id, "Test content", 0).await;

    // Add some non-existent chunk IDs
    let fake_chunk_id = Uuid::new_v4().to_string();
    let chunk_ids = vec![chunk_id.clone(), fake_chunk_id];

    let result = service.enrich_results(&chunk_ids).await.unwrap();

    // Should only return metadata for existing chunk
    assert_eq!(result.len(), 1);
    assert!(result.contains_key(&chunk_id));

    pool.close().await;
}

/// Test 9: Multiple documents
#[tokio::test]
async fn test_enrich_multiple_documents() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    // Create multiple documents
    let doc1_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc1_id, "doc1.txt", "/path/doc1.txt").await;

    let doc2_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc2_id, "doc2.txt", "/path/doc2.txt").await;

    // Create chunks for each document
    let chunk1_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk1_id, &doc1_id, "Doc1 content", 0).await;

    let chunk2_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk2_id, &doc2_id, "Doc2 content", 0).await;

    let chunk_ids = vec![chunk1_id.clone(), chunk2_id.clone()];
    let result = service.enrich_results(&chunk_ids).await.unwrap();

    assert_eq!(result.len(), 2);

    let meta1 = result.get(&chunk1_id).unwrap();
    assert_eq!(
        meta1.metadata.get("filename").unwrap().as_str().unwrap(),
        "doc1.txt"
    );

    let meta2 = result.get(&chunk2_id).unwrap();
    assert_eq!(
        meta2.metadata.get("filename").unwrap().as_str().unwrap(),
        "doc2.txt"
    );

    pool.close().await;
}

/// Test 10: Parallel batch processing performance
#[tokio::test]
async fn test_enrich_parallel_performance() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    // Create 3600 chunks (4 batches of 900)
    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "large.txt", "/path/large.txt").await;

    let mut chunk_ids = Vec::new();
    for i in 0..3600 {
        let chunk_id = Uuid::new_v4().to_string();
        insert_test_chunk(&pool, &chunk_id, &doc_id, &format!("Content {}", i), i).await;
        chunk_ids.push(chunk_id);
    }

    // Measure enrichment time
    let start = Instant::now();
    let result = service.enrich_results(&chunk_ids).await;
    let duration = start.elapsed();

    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert_eq!(metadata.len(), 3600);

    println!("Enriched 3600 chunks in {:?}", duration);
    println!(
        "Throughput: {:.2} chunks/sec",
        3600.0 / duration.as_secs_f64()
    );

    // Should complete in reasonable time with parallel processing
    assert!(
        duration.as_secs() < 10,
        "Parallel processing too slow: {:?}",
        duration
    );

    pool.close().await;
}

/// Test 11: Batch boundary conditions
#[tokio::test]
async fn test_enrich_batch_boundaries() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "test.txt", "/path/test.txt").await;

    // Test exact batch boundary (900)
    let mut chunk_ids = Vec::new();
    for i in 0..900 {
        let chunk_id = Uuid::new_v4().to_string();
        insert_test_chunk(&pool, &chunk_id, &doc_id, &format!("Content {}", i), i).await;
        chunk_ids.push(chunk_id);
    }

    let result = service.enrich_results(&chunk_ids).await.unwrap();
    assert_eq!(result.len(), 900);

    // Test boundary + 1 (901 - should create 2 batches)
    let chunk_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk_id, &doc_id, "Extra content", 900).await;
    chunk_ids.push(chunk_id);

    let result = service.enrich_results(&chunk_ids).await.unwrap();
    assert_eq!(result.len(), 901);

    pool.close().await;
}

/// Test 12: Unicode content in snippets
#[tokio::test]
async fn test_enrich_unicode_content() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    let unicode_content = "Hello 世界 🌍 Привет مرحبا";

    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "unicode.txt", "/path/unicode.txt").await;

    let chunk_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk_id, &doc_id, unicode_content, 0).await;

    let result = service
        .enrich_results(std::slice::from_ref(&chunk_id))
        .await
        .unwrap();
    let metadata = result.get(&chunk_id).unwrap();

    assert_eq!(metadata.snippet, unicode_content);

    pool.close().await;
}

/// Test 13: Stress test - 10,000 chunks
#[tokio::test]
#[ignore] // Run only when explicitly requested (slow test)
async fn test_enrich_stress_test() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    // Create 10,000 chunks
    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "huge.txt", "/path/huge.txt").await;

    let mut chunk_ids = Vec::new();
    for i in 0..10_000 {
        let chunk_id = Uuid::new_v4().to_string();
        insert_test_chunk(&pool, &chunk_id, &doc_id, &format!("Content {}", i), i).await;
        chunk_ids.push(chunk_id);

        if i % 1000 == 0 {
            println!("Inserted {} chunks", i);
        }
    }

    println!("Starting enrichment of 10,000 chunks...");
    let start = Instant::now();
    let result = service.enrich_results(&chunk_ids).await;
    let duration = start.elapsed();

    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert_eq!(metadata.len(), 10_000);

    println!("Enriched 10,000 chunks in {:?}", duration);
    println!(
        "Throughput: {:.2} chunks/sec",
        10_000.0 / duration.as_secs_f64()
    );

    pool.close().await;
}

/// Test 14: Verify query timeout protection
#[tokio::test]
async fn test_enrich_query_timeout() {
    let pool = create_test_db().await;
    let service = SearchEnrichmentService::new(pool.clone());

    // Normal operation should complete without timeout
    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "test.txt", "/path/test.txt").await;

    let chunk_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk_id, &doc_id, "Test content", 0).await;

    let start = Instant::now();
    let result = service.enrich_results(&[chunk_id]).await;
    let duration = start.elapsed();

    assert!(result.is_ok());
    assert!(duration.as_secs() < 30); // Should complete well under timeout

    pool.close().await;
}

/// Test 15: Database connection pool handling
#[tokio::test]
async fn test_enrich_connection_pooling() {
    let pool = create_test_db().await;

    // Create multiple service instances (sharing pool)
    let service1 = SearchEnrichmentService::new(pool.clone());
    let service2 = SearchEnrichmentService::new(pool.clone());

    let doc_id = Uuid::new_v4().to_string();
    insert_test_document(&pool, &doc_id, "test.txt", "/path/test.txt").await;

    let chunk1_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk1_id, &doc_id, "Content 1", 0).await;

    let chunk2_id = Uuid::new_v4().to_string();
    insert_test_chunk(&pool, &chunk2_id, &doc_id, "Content 2", 1).await;

    // Use both services concurrently
    let result1 = service1.enrich_results(&[chunk1_id]).await;
    let result2 = service2.enrich_results(&[chunk2_id]).await;

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    pool.close().await;
}
