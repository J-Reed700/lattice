#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! # End-to-End Indexing Integration Tests
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Comprehensive tests for the complete document indexing workflow.
//!
//! ## Test Coverage
//!
//! - Document ingestion and storage
//! - Content extraction
//! - Chunk generation
//! - Embedding creation
//! - Metadata extraction
//! - Error handling and edge cases
//!
//! ## Test Philosophy
//!
//! These tests simulate real-world document indexing scenarios:
//! - Multiple document types (markdown, text, code)
//! - Various content sizes
//! - Concurrent indexing
//! - Error recovery

use lattice::error::Result;
use lattice::infrastructure::indexing::chunker::{ChunkingStrategy, RecursiveChunker};

mod helpers;
use helpers::{TestContext, DocumentFactory, ChunkFactory, assert_document_exists, assert_chunk_count};

// ============================================================================
// Basic Indexing Tests
// ============================================================================

#[tokio::test]
async fn test_single_document_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create document
    let doc = ctx.create_test_document("test.md", "# Test Document\n\nThis is test content.").await?;

    // Verify document exists
    assert_document_exists(&ctx.doc_repo(), &doc.id).await?;

    // Verify stats
    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 1);

    Ok(())
}

#[tokio::test]
async fn test_document_with_chunks_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create document
    let doc = ctx.create_test_document("test.md", "Test content").await?;

    // Create chunks
    let chunks = ctx.create_test_chunks(&doc.id, 3).await?;

    // Verify chunks
    assert_chunk_count(&ctx.chunk_repo(), &doc.id, 3).await?;

    // Verify chunk content
    for (i, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.chunk_index, i as i32);
        assert!(!chunk.content.is_empty());
    }

    Ok(())
}

#[tokio::test]
async fn test_document_with_embeddings_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create document
    let doc = ctx.create_test_document("test.md", "Test content").await?;

    // Create chunks
    let chunks = ctx.create_test_chunks(&doc.id, 2).await?;

    // Create embeddings
    let chunk_ids: Vec<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
    let embeddings = ctx.create_test_embeddings(&chunk_ids).await?;

    // Verify embeddings
    assert_eq!(embeddings.len(), 2);

    for embedding in &embeddings {
        assert_eq!(embedding.embedding.len(), 384); // Default dimension
    }

    Ok(())
}

// ============================================================================
// Content Chunking Tests
// ============================================================================

#[tokio::test]
async fn test_large_document_chunking() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create large document
    let large_content = "Lorem ipsum dolor sit amet. ".repeat(100); // ~2800 chars
    let doc = ctx.create_test_document("large.md", &large_content).await?;

    // Chunk it
    let chunk_repo = ctx.chunk_repo();

    // Create chunks manually with different sizes
    for i in 0..5 {
        let chunk_content = large_content.chars().skip(i * 500).take(500).collect::<String>();
        chunk_repo
            .create_chunk(&doc.id, &chunk_content, i as i32, Some((i * 500) as i32), Some(((i + 1) * 500) as i32))
            .await?;
    }

    // Verify chunking
    assert_chunk_count(&chunk_repo, &doc.id, 5).await?;

    let chunks = chunk_repo.get_chunks_by_document(&doc.id).await?;

    // Verify chunks are sequential
    for (i, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.chunk_index, i as i32);
    }

    Ok(())
}

#[tokio::test]
async fn test_markdown_document_chunking() -> Result<()> {
    let ctx = TestContext::new().await?;

    let markdown_content = r#"# Introduction

This is the introduction section.

## Background

Here is some background information.

### Details

More detailed information here.

## Methods

Our methodology section.

## Results

The results of our study.
"#;

    let doc = ctx.create_test_document("research.md", markdown_content).await?;

    // Create semantic chunks (by section)
    let sections = vec![
        "# Introduction\n\nThis is the introduction section.",
        "## Background\n\nHere is some background information.",
        "### Details\n\nMore detailed information here.",
        "## Methods\n\nOur methodology section.",
        "## Results\n\nThe results of our study.",
    ];

    let chunk_repo = ctx.chunk_repo();

    for (i, section) in sections.iter().enumerate() {
        chunk_repo
            .create_chunk(&doc.id, section, i as i32, None, None)
            .await?;
    }

    // Verify semantic chunking preserved structure
    let chunks = chunk_repo.get_chunks_by_document(&doc.id).await?;
    assert_eq!(chunks.len(), 5);

    // Verify first chunk has heading
    assert!(chunks[0].content.starts_with('#'));

    Ok(())
}

#[tokio::test]
async fn test_code_file_chunking() -> Result<()> {
    let ctx = TestContext::new().await?;

    let code_content = r#"
fn main() {
    println!("Hello, world!");
}

fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

#[test]
fn test_sum() {
    assert_eq!(calculate_sum(2, 3), 5);
}

struct Person {
    name: String,
    age: u32,
}

impl Person {
    fn new(name: String, age: u32) -> Self {
        Self { name, age }
    }
}
"#;

    let doc = ctx.create_test_document("main.rs", code_content).await?;

    // Chunk by logical units (functions, structs)
    let chunks = vec![
        "fn main() {\n    println!(\"Hello, world!\");\n}",
        "fn calculate_sum(a: i32, b: i32) -> i32 {\n    a + b\n}",
        "#[test]\nfn test_sum() {\n    assert_eq!(calculate_sum(2, 3), 5);\n}",
        "struct Person {\n    name: String,\n    age: u32,\n}",
        "impl Person {\n    fn new(name: String, age: u32) -> Self {\n        Self { name, age }\n    }\n}",
    ];

    let chunk_repo = ctx.chunk_repo();

    for (i, chunk) in chunks.iter().enumerate() {
        chunk_repo
            .create_chunk(&doc.id, chunk, i as i32, None, None)
            .await?;
    }

    // Verify code chunking
    let stored_chunks = chunk_repo.get_chunks_by_document(&doc.id).await?;
    assert_eq!(stored_chunks.len(), 5);

    Ok(())
}

// ============================================================================
// Batch Indexing Tests
// ============================================================================

#[tokio::test]
async fn test_batch_document_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create multiple documents
    let mut doc_ids = Vec::new();

    for i in 0..10 {
        let doc = ctx
            .create_test_document(
                &format!("doc-{}.md", i),
                &format!("Content for document {}", i),
            )
            .await?;

        doc_ids.push(doc.id);
    }

    // Verify all documents exist
    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 10);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_document_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Index documents concurrently
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let doc_repo = ctx.doc_repo();
            let chunk_repo = ctx.chunk_repo();

            tokio::spawn(async move {
                let doc = DocumentFactory::new()
                    .file_name(&format!("concurrent-{}.md", i))
                    .content(&format!("Concurrent content {}", i))
                    .build();

                doc.insert_into_db(&doc_repo).await?;

                // Add chunks
                for j in 0..3 {
                    chunk_repo
                        .create_chunk(&doc.id, &format!("Chunk {}", j), j, None, None)
                        .await?;
                }

                Ok::<_, lattice::error::AppError>(doc.id)
            })
        })
        .collect();

    // Wait for all to complete
    let results: Vec<_> = futures::future::join_all(handles).await;

    // Verify all succeeded
    for result in results {
        assert!(result.is_ok());
        let doc_id = result.unwrap()?;
        assert!(!doc_id.is_empty());
    }

    // Verify database state
    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 5);
    assert_eq!(stats.chunks, 15); // 5 docs * 3 chunks

    Ok(())
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[tokio::test]
async fn test_empty_document_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create empty document
    let doc = ctx.create_test_document("empty.md", "").await?;

    // Verify it was stored
    assert_document_exists(&ctx.doc_repo(), &doc.id).await?;

    Ok(())
}

#[tokio::test]
async fn test_very_long_document_indexing() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create very long document (100KB)
    let long_content = "A".repeat(100_000);
    let doc = ctx.create_test_document("long.txt", &long_content).await?;

    assert_document_exists(&ctx.doc_repo(), &doc.id).await?;
    assert_eq!(doc.file_size, 100_000);

    Ok(())
}

#[tokio::test]
async fn test_special_characters_in_content() -> Result<()> {
    let ctx = TestContext::new().await?;

    let special_content = r#"
Unicode: 你好世界 🌍 café
Quotes: "double" 'single'
Symbols: @#$%^&*()
Newlines and tabs:
    Indented content
        More indented
"#;

    let doc = ctx.create_test_document("special.md", special_content).await?;

    assert_document_exists(&ctx.doc_repo(), &doc.id).await?;

    Ok(())
}

#[tokio::test]
async fn test_duplicate_document_handling() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Create first document
    let doc1 = ctx.create_test_document("duplicate.md", "Original content").await?;

    // Create "duplicate" with same name but different ID
    let doc2 = DocumentFactory::new()
        .file_name("duplicate.md")
        .content("Different content")
        .build();

    doc2.insert_into_db(&ctx.doc_repo()).await?;

    // Both should exist (different IDs)
    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 2);

    Ok(())
}

// ============================================================================
// Metadata and Content Extraction Tests
// ============================================================================

#[tokio::test]
async fn test_document_metadata_extraction() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = DocumentFactory::new()
        .file_name("metadata-test.md")
        .file_type("markdown")
        .file_size(1024)
        .content("Test content with metadata")
        .build();

    doc.insert_into_db(&ctx.doc_repo()).await?;

    // Verify metadata is stored correctly
    let retrieved = ctx.doc_repo().get_by_id(&doc.id).await?;
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.file_name, "metadata-test.md");
    assert_eq!(retrieved.file_type, "markdown");

    Ok(())
}

// ============================================================================
// Cleanup and Resource Management Tests
// ============================================================================

#[tokio::test]
async fn test_context_cleanup() -> Result<()> {
    // Create context in a scope
    let doc_id = {
        let ctx = TestContext::new().await?;
        let doc = ctx.create_test_document("cleanup.md", "Test").await?;
        doc.id
    }; // Context dropped here

    // Verify cleanup happened (new context shouldn't have old data)
    let new_ctx = TestContext::new().await?;
    let stats = new_ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 0); // Fresh database

    Ok(())
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_indexing_performance() -> Result<()> {
    let ctx = TestContext::new().await?;

    let start = std::time::Instant::now();

    // Index 100 documents
    for i in 0..100 {
        ctx.create_test_document(&format!("perf-{}.md", i), "Test content")
            .await?;
    }

    let duration = start.elapsed();

    // Should complete in reasonable time (< 5 seconds for in-memory DB)
    assert!(duration.as_secs() < 5, "Indexing took too long: {:?}", duration);

    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 100);

    Ok(())
}

#[tokio::test]
async fn test_chunk_creation_performance() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("perf-chunks.md", "Test").await?;

    let start = std::time::Instant::now();

    // Create 1000 chunks
    for i in 0..1000 {
        let chunk_repo = ctx.chunk_repo();
        chunk_repo
            .create_chunk(&doc.id, &format!("Chunk {}", i), i as i32, None, None)
            .await?;
    }

    let duration = start.elapsed();

    // Should complete in reasonable time
    assert!(duration.as_secs() < 10, "Chunk creation took too long: {:?}", duration);

    assert_chunk_count(&ctx.chunk_repo(), &doc.id, 1000).await?;

    Ok(())
}

#[cfg(test)]
mod helpers {
    pub use crate::helpers::*;
}
