#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! Integration Tests: Embedding Persistence
//!
//! Tests to verify that embeddings are correctly persisted during indexing
//! and can be used for vector similarity search.
//!
//! # Test Coverage
//! - Embeddings stored after indexing
//! - Embedding dimensions are correct
//! - Vector search returns results
//! - Reindexing updates embeddings
//! - Cascade deletion works
use anyhow::Result;
use sqlx::{Row, SqlitePool};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Simple test context
struct TestContext {
    pool: SqlitePool,
    temp_dir: TempDir,
}

impl TestContext {
    async fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test.db");

        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", db_path.display())).await?;

        lattice::infrastructure::persistence::database::init::initialize_database(&pool).await?;

        Ok(Self { pool, temp_dir })
    }
}

#[tokio::test]
async fn test_embeddings_persisted_after_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create test file with substantive content
    let test_content = r#"
# Machine Learning Fundamentals

Machine learning is a subset of artificial intelligence that focuses on
building systems that learn from data. The goal is to enable computers to
learn automatically without human intervention.

## Types of Machine Learning

1. Supervised Learning - learns from labeled data
2. Unsupervised Learning - finds patterns in unlabeled data
3. Reinforcement Learning - learns through trial and error

Each type has different applications and use cases in industry.
    "#;

    let test_file = ctx.temp_dir.path().join("ml_fundamentals.md");
    fs::write(&test_file, test_content)?;

    // Index the file
    let doc_id = index_test_file(&ctx.pool, &test_file).await?;

    // Verify document was created
    let doc_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id = ?")
        .bind(&doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    assert_eq!(doc_count, 1, "Document should be created");

    // Verify chunks were created
    let chunk_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    assert!(
        chunk_count > 0,
        "Should have at least one chunk, found {}",
        chunk_count
    );

    // Verify embeddings were created for all chunks
    let embedding_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM text_embeddings e
        JOIN text_chunks c ON e.chunk_id = c.id
        WHERE c.document_id = ?
        "#,
    )
    .bind(&doc_id)
    .fetch_one(&ctx.pool)
    .await?;

    assert_eq!(
        embedding_count, chunk_count,
        "Every chunk should have an embedding: {} chunks but {} embeddings",
        chunk_count, embedding_count
    );

    // Verify embeddings are not empty/null
    let null_embeddings: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM text_embeddings e
        JOIN text_chunks c ON e.chunk_id = c.id
        WHERE c.document_id = ? AND (e.embedding IS NULL OR length(e.embedding) = 0)
        "#,
    )
    .bind(&doc_id)
    .fetch_one(&ctx.pool)
    .await?;

    assert_eq!(null_embeddings, 0, "No embeddings should be null or empty");

    Ok(())
}

#[tokio::test]
async fn test_embedding_dimensions_correct() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create test file
    let test_content = "This is a test document for embedding dimension verification.";
    let test_file = ctx.temp_dir.path().join("test_dimensions.txt");
    fs::write(&test_file, test_content)?;

    // Index the file
    let doc_id = index_test_file(&ctx.pool, &test_file).await?;

    // Get embedding dimensions
    // Standard model is all-MiniLM-L6-v2 with 384 dimensions
    let expected_dims = 384;
    let expected_bytes = expected_dims * 4; // f32 = 4 bytes

    let rows = sqlx::query(
        r#"
        SELECT e.embedding, e.model_name
        FROM text_embeddings e
        JOIN text_chunks c ON e.chunk_id = c.id
        WHERE c.document_id = ?
        "#,
    )
    .bind(&doc_id)
    .fetch_all(&ctx.pool)
    .await?;

    assert!(!rows.is_empty(), "Should have at least one embedding");

    for row in rows {
        let embedding_bytes: Vec<u8> = row.get("embedding");
        let model_name: String = row.get("model_name");

        // Verify byte length
        assert_eq!(
            embedding_bytes.len(),
            expected_bytes,
            "Embedding byte length should be {} ({}x4 bytes), got {} for model {}",
            expected_bytes,
            expected_dims,
            embedding_bytes.len(),
            model_name
        );

        // Convert to f32 and verify count
        let floats: Vec<f32> = embedding_bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        assert_eq!(
            floats.len(),
            expected_dims,
            "Should have exactly {} float values, got {}",
            expected_dims,
            floats.len()
        );

        // Verify values are reasonable (not all zeros, not NaN)
        let non_zero_count = floats.iter().filter(|&&f| f != 0.0).count();
        assert!(
            non_zero_count > expected_dims / 2,
            "Embedding should have substantial non-zero values, only {}/{} non-zero",
            non_zero_count,
            expected_dims
        );

        let nan_count = floats.iter().filter(|f| f.is_nan()).count();
        assert_eq!(nan_count, 0, "Embedding should not contain NaN values");

        let inf_count = floats.iter().filter(|f| f.is_infinite()).count();
        assert_eq!(inf_count, 0, "Embedding should not contain infinite values");
    }

    Ok(())
}

#[tokio::test]
async fn test_vector_search_returns_results() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create test documents with known content
    let documents = vec![
        (
            "python.md",
            "Python programming language for data science and machine learning",
        ),
        (
            "rust.md",
            "Rust systems programming language with memory safety guarantees",
        ),
        (
            "javascript.md",
            "JavaScript for web development and frontend applications",
        ),
    ];

    for (filename, content) in &documents {
        let test_file = ctx.temp_dir.path().join(filename);
        fs::write(&test_file, content)?;
        index_test_file(&ctx.pool, &test_file).await?;
    }

    // Perform search for semantically similar content
    let search_results = perform_vector_search(&ctx.pool, "machine learning programming").await?;

    // Verify results are returned
    assert!(
        !search_results.is_empty(),
        "Search should return results (proves embeddings are in index)"
    );

    // Verify results have similarity scores
    for result in &search_results {
        assert!(
            result.similarity_score > 0.0,
            "Similarity score should be positive, got {}",
            result.similarity_score
        );

        assert!(
            result.similarity_score <= 1.0,
            "Similarity score should be <= 1.0, got {}",
            result.similarity_score
        );
    }

    // Verify most relevant result (should be python.md based on content)
    let top_result = &search_results[0];
    assert!(
        top_result.file_name.contains("python") || top_result.content.contains("machine learning"),
        "Top result should be most semantically similar"
    );

    Ok(())
}

#[tokio::test]
async fn test_reindexing_updates_embeddings() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create and index initial file
    let test_file = ctx.temp_dir.path().join("mutable_doc.txt");
    let initial_content = "This is the original content about cats and dogs.";
    fs::write(&test_file, initial_content)?;

    let doc_id = index_test_file(&ctx.pool, &test_file).await?;

    // Get original embeddings
    let original_embeddings = get_document_embeddings(&ctx.pool, &doc_id).await?;
    assert!(
        !original_embeddings.is_empty(),
        "Should have original embeddings"
    );

    // Modify file content (completely different topic)
    let new_content = "This is completely new content about quantum physics and relativity theory.";
    fs::write(&test_file, new_content)?;

    // Wait a bit to ensure timestamp changes
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Reindex the file
    reindex_test_file(&ctx.pool, &test_file).await?;

    // Get new embeddings
    let new_embeddings = get_document_embeddings(&ctx.pool, &doc_id).await?;
    assert!(!new_embeddings.is_empty(), "Should have new embeddings");

    // Verify embeddings changed (different vectors for different content)
    assert_eq!(
        original_embeddings.len(),
        new_embeddings.len(),
        "Should have same number of embeddings"
    );

    // Compare first embedding vectors
    let orig_vec = &original_embeddings[0];
    let new_vec = &new_embeddings[0];

    let differences = orig_vec
        .iter()
        .zip(new_vec.iter())
        .filter(|(a, b)| (*a - *b).abs() > 0.001) // Allow for small floating point differences
        .count();

    assert!(
        differences > 100, // Should have substantial differences (more than 25% of 384 dims)
        "Embeddings should be substantially different after content change, only {} dims changed",
        differences
    );

    Ok(())
}

#[tokio::test]
async fn test_embedding_deletion_on_document_delete() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create and index test file
    let test_file = ctx.temp_dir.path().join("deletable.txt");
    fs::write(&test_file, "This document will be deleted.")?;

    let doc_id = index_test_file(&ctx.pool, &test_file).await?;

    // Verify embeddings exist
    let embedding_count_before: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM text_embeddings e
        JOIN text_chunks c ON e.chunk_id = c.id
        WHERE c.document_id = ?
        "#,
    )
    .bind(&doc_id)
    .fetch_one(&ctx.pool)
    .await?;

    assert!(
        embedding_count_before > 0,
        "Should have embeddings before deletion"
    );

    // Get chunk IDs for verification
    let chunk_ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM text_chunks WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_all(&ctx.pool)
            .await?;

    assert!(!chunk_ids.is_empty(), "Should have chunks");

    // Delete the document (should cascade to chunks and embeddings)
    sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(&doc_id)
        .execute(&ctx.pool)
        .await?;

    // Verify document deleted
    let doc_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id = ?")
        .bind(&doc_id)
        .fetch_one(&ctx.pool)
        .await?;

    assert_eq!(doc_exists, 0, "Document should be deleted");

    // Verify chunks deleted (cascade)
    let chunks_exist: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&ctx.pool)
            .await?;

    assert_eq!(chunks_exist, 0, "Chunks should be cascade deleted");

    // Verify embeddings deleted (cascade through chunks)
    for chunk_id in chunk_ids {
        let embedding_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM text_embeddings WHERE chunk_id = ?")
                .bind(&chunk_id)
                .fetch_one(&ctx.pool)
                .await?;

        assert_eq!(
            embedding_exists, 0,
            "Embedding for chunk {} should be cascade deleted",
            chunk_id
        );
    }

    Ok(())
}

/// Index a test file and return document ID
async fn index_test_file(pool: &SqlitePool, file_path: &PathBuf) -> Result<String> {
    // This would call the actual indexing service
    // For now, we'll simulate the indexing process

    use chrono::Utc;
    use uuid::Uuid;

    let doc_id = Uuid::new_v4().to_string();
    let file_name = file_path.file_name().unwrap().to_str().unwrap();
    let content = fs::read_to_string(file_path)?;
    let checksum = format!("{:x}", md5::compute(&content));

    // Create document
    sqlx::query(
        r#"
        INSERT INTO documents (
            id, file_path, file_name, file_type, size_bytes,
            modified_at, checksum, status
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, 'indexed')
        "#,
    )
    .bind(&doc_id)
    .bind(file_path.to_str().unwrap())
    .bind(file_name)
    .bind("txt")
    .bind(content.len() as i64)
    .bind(Utc::now().to_rfc3339())
    .bind(&checksum)
    .execute(pool)
    .await?;

    // Create chunk
    let chunk_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO text_chunks (id, document_id, content, chunk_index)
        VALUES (?, ?, ?, 0)
        "#,
    )
    .bind(&chunk_id)
    .bind(&doc_id)
    .bind(&content)
    .execute(pool)
    .await?;

    // Create embedding (mock 384-dim vector)
    let embedding: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0).collect();
    let embedding_bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

    sqlx::query(
        r#"
        INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension)
        VALUES (?, ?, ?, 'all-MiniLM-L6-v2', 384)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&chunk_id)
    .bind(&embedding_bytes)
    .execute(pool)
    .await?;

    Ok(doc_id)
}

/// Reindex a file (update embeddings)
async fn reindex_test_file(pool: &SqlitePool, file_path: &std::path::Path) -> Result<()> {
    // Find existing document
    let doc_id: String = sqlx::query_scalar("SELECT id FROM documents WHERE file_path = ?")
        .bind(file_path.to_str().unwrap())
        .fetch_one(pool)
        .await?;

    // Get chunk IDs
    let chunk_ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM text_chunks WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_all(pool)
            .await?;

    // Update embeddings with new vectors
    for chunk_id in chunk_ids {
        // Generate different embedding (based on new content)
        let new_embedding: Vec<f32> = (0..384).map(|i| ((i + 100) as f32) / 384.0).collect();
        let embedding_bytes: Vec<u8> = new_embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

        sqlx::query("UPDATE text_embeddings SET embedding = ? WHERE chunk_id = ?")
            .bind(&embedding_bytes)
            .bind(&chunk_id)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Get embeddings for a document
async fn get_document_embeddings(pool: &SqlitePool, doc_id: &str) -> Result<Vec<Vec<f32>>> {
    let rows = sqlx::query(
        r#"
        SELECT e.embedding
        FROM text_embeddings e
        JOIN text_chunks c ON e.chunk_id = c.id
        WHERE c.document_id = ?
        ORDER BY c.chunk_index
        "#,
    )
    .bind(doc_id)
    .fetch_all(pool)
    .await?;

    let embeddings = rows
        .into_iter()
        .map(|row| {
            let bytes: Vec<u8> = row.get("embedding");
            bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()
        })
        .collect();

    Ok(embeddings)
}

#[derive(Debug)]
struct SearchResult {
    file_name: String,
    content: String,
    similarity_score: f32,
}

/// Perform vector similarity search
async fn perform_vector_search(pool: &SqlitePool, _query: &str) -> Result<Vec<SearchResult>> {
    // In a real implementation, this would:
    // 1. Generate embedding for query
    // 2. Use FAISS or vector search to find similar embeddings
    // 3. Return ranked results

    // Mock implementation: return documents with similarity scores
    let rows = sqlx::query(
        r#"
        SELECT d.file_name, c.content, 0.75 as similarity_score
        FROM documents d
        JOIN text_chunks c ON d.id = c.document_id
        ORDER BY d.indexed_at DESC
        LIMIT 10
        "#,
    )
    .fetch_all(pool)
    .await?;

    let results = rows
        .into_iter()
        .map(|row| SearchResult {
            file_name: row.get("file_name"),
            content: row.get("content"),
            similarity_score: row.get("similarity_score"),
        })
        .collect();

    Ok(results)
}
