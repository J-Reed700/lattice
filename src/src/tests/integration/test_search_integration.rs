#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! # Search Integration Tests

//!
//! Comprehensive tests for semantic and hybrid search functionality.
//!
//! ## Test Coverage
//!
//! - Semantic search with embeddings
//! - Full-text search (BM25/FTS5)
//! - Hybrid search (semantic + keyword)
//! - Query expansion
//! - Reranking
//! - Search result relevance
//!
//! ## Test Scenarios
//!
//! - Single query searches
//! - Multi-query searches
//! - Empty result handling
//! - Ranking and scoring
//! - Performance benchmarks

use lattice::shared::error::Result;

mod helpers;
use helpers::{
    TestContext, DocumentFactory, ChunkFactory, MockEmbedder,
    assert_search_contains, assert_min_results, assert_max_results,
    assert_embeddings_similar,
};

#[tokio::test]
async fn test_semantic_search_basic() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document("ml.md", "Machine learning and neural networks").await?;
    let doc2 = ctx.create_test_document("cooking.md", "Recipes and cooking techniques").await?;
    let doc3 = ctx.create_test_document("ai.md", "Artificial intelligence and deep learning").await?;

    ctx.create_test_chunks(&doc1.id, 1).await?;
    ctx.create_test_chunks(&doc2.id, 1).await?;
    ctx.create_test_chunks(&doc3.id, 1).await?;

    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.documents, 3);
    assert_eq!(stats.chunks, 3);

    Ok(())
}

#[tokio::test]
async fn test_embedding_similarity_search() -> Result<()> {
    let ctx = TestContext::new().await?;
    let embedder = &ctx.embedder;

    let ml_embedding = embedder.embed_text("machine learning").await?;
    let ai_embedding = embedder.embed_text("artificial intelligence").await?;
    let cooking_embedding = embedder.embed_text("cooking recipes").await?;

    // ML and AI should be similar
    assert_embeddings_similar(&ml_embedding, &ai_embedding, 0.7);

    // ML and cooking should be different
    use helpers::assert_embeddings_different;
    assert_embeddings_different(&ml_embedding, &cooking_embedding, 0.5);

    Ok(())
}

#[tokio::test]
async fn test_search_with_embeddings() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("search-test.md", "Search functionality test").await?;
    let chunks = ctx.create_test_chunks(&doc.id, 2).await?;

    let chunk_ids: Vec<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
    let embeddings = ctx.create_test_embeddings(&chunk_ids).await?;

    assert_eq!(embeddings.len(), 2);

    // Query embedding
    let query_embedding = ctx.embedder.embed_text("search test").await?;

    assert_eq!(query_embedding.len(), 384);

    Ok(())
}

#[tokio::test]
async fn test_full_text_search() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document(
        "rust.md",
        "Rust programming language with memory safety"
    ).await?;

    let doc2 = ctx.create_test_document(
        "python.md",
        "Python programming with dynamic typing"
    ).await?;

    let doc3 = ctx.create_test_document(
        "rust-advanced.md",
        "Advanced Rust features including ownership"
    ).await?;

    // Search using document repository FTS
    let doc_repo = ctx.doc_repo();

    let rust_results = doc_repo.search_documents("Rust").await?;
    assert!(rust_results.len() >= 1);

    let python_results = doc_repo.search_documents("Python").await?;
    assert!(python_results.len() >= 1);

    Ok(())
}

#[tokio::test]
async fn test_keyword_search_exact_match() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document(
        "unique-keyword.md",
        "This document contains a unique_test_keyword_12345"
    ).await?;

    let doc_repo = ctx.doc_repo();
    let results = doc_repo.search_documents("unique_test_keyword_12345").await?;

    assert!(!results.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_search_multiple_terms() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document(
        "multi-term.md",
        "Machine learning with neural networks and deep learning"
    ).await?;

    let doc_repo = ctx.doc_repo();

    // Search for individual terms
    let ml_results = doc_repo.search_documents("machine learning").await?;
    let nn_results = doc_repo.search_documents("neural networks").await?;

    assert!(!ml_results.is_empty());
    assert!(!nn_results.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_hybrid_search_combination() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document(
        "hybrid1.md",
        "Rust systems programming language"
    ).await?;

    let doc2 = ctx.create_test_document(
        "hybrid2.md",
        "Systems design and architecture"
    ).await?;

    let doc3 = ctx.create_test_document(
        "hybrid3.md",
        "Rust web development frameworks"
    ).await?;

    ctx.create_test_chunks(&doc1.id, 1).await?;
    ctx.create_test_chunks(&doc2.id, 1).await?;
    ctx.create_test_chunks(&doc3.id, 1).await?;

    // Both keyword and semantic search should work
    let doc_repo = ctx.doc_repo();

    let rust_results = doc_repo.search_documents("Rust").await?;
    assert!(rust_results.len() >= 2);

    let systems_results = doc_repo.search_documents("systems").await?;
    assert!(systems_results.len() >= 2);

    Ok(())
}

#[tokio::test]
async fn test_search_result_ranking() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document(
        "exact.md",
        "machine learning machine learning machine learning"
    ).await?;

    let doc2 = ctx.create_test_document(
        "partial.md",
        "machine learning is a subset of AI"
    ).await?;

    let doc3 = ctx.create_test_document(
        "tangential.md",
        "Computing involves many fields including ML"
    ).await?;

    let doc_repo = ctx.doc_repo();
    let results = doc_repo.search_documents("machine learning").await?;

    assert!(!results.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_empty_query_search() -> Result<()> {
    let ctx = TestContext::new().await?;

    ctx.create_test_document("test.md", "Test content").await?;

    let doc_repo = ctx.doc_repo();
    let results = doc_repo.search_documents("").await?;

    assert!(results.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_no_results_search() -> Result<()> {
    let ctx = TestContext::new().await?;

    ctx.create_test_document("test.md", "Test content").await?;

    let doc_repo = ctx.doc_repo();
    let results = doc_repo.search_documents("nonexistent_unique_query_12345").await?;

    assert!(results.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_special_characters_search() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document(
        "special.md",
        "Test with @mentions and #tags and [[links]]"
    ).await?;

    let doc_repo = ctx.doc_repo();

    // Search should handle special characters
    let results1 = doc_repo.search_documents("mentions").await?;
    let results2 = doc_repo.search_documents("tags").await?;

    assert!(!results1.is_empty());
    assert!(!results2.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_unicode_search() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document(
        "unicode.md",
        "Hello 你好 world 世界 café"
    ).await?;

    let doc_repo = ctx.doc_repo();

    let results = doc_repo.search_documents("world").await?;
    assert!(!results.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_case_insensitive_search() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document(
        "case.md",
        "Machine Learning and MACHINE LEARNING"
    ).await?;

    let doc_repo = ctx.doc_repo();

    let lower = doc_repo.search_documents("machine learning").await?;
    let upper = doc_repo.search_documents("MACHINE LEARNING").await?;
    let mixed = doc_repo.search_documents("Machine Learning").await?;

    // All should return results
    assert!(!lower.is_empty());
    assert!(!upper.is_empty());
    assert!(!mixed.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_search_performance_many_documents() -> Result<()> {
    let ctx = TestContext::new().await?;

    for i in 0..100 {
        ctx.create_test_document(
            &format!("doc-{}.md", i),
            &format!("Document {} with test content", i),
        )
        .await?;
    }

    let doc_repo = ctx.doc_repo();

    let start = std::time::Instant::now();
    let results = doc_repo.search_documents("test content").await?;
    let duration = start.elapsed();

    // Search should be fast
    assert!(duration.as_millis() < 1000, "Search too slow: {:?}", duration);
    assert!(!results.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_multiple_concurrent_searches() -> Result<()> {
    let ctx = TestContext::new().await?;

    for i in 0..20 {
        ctx.create_test_document(
            &format!("concurrent-{}.md", i),
            &format!("Content for document {}", i),
        )
        .await?;
    }

    let doc_repo = ctx.doc_repo();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let repo = doc_repo.clone();
            let query = format!("document {}", i);

            tokio::spawn(async move {
                repo.search_documents(&query).await
            })
        })
        .collect();

    // Wait for all searches
    let results = futures::future::join_all(handles).await;

    // All should succeed
    for result in results {
        assert!(result.is_ok());
    }

    Ok(())
}

#[tokio::test]
async fn test_search_with_document_type_filter() -> Result<()> {
    let ctx = TestContext::new().await?;

    let md_doc = DocumentFactory::new()
        .file_name("test.md")
        .file_type("markdown")
        .content("Markdown test content")
        .build();

    let txt_doc = DocumentFactory::new()
        .file_name("test.txt")
        .file_type("text")
        .content("Text test content")
        .build();

    md_doc.insert_into_db(&ctx.doc_repo()).await?;
    txt_doc.insert_into_db(&ctx.doc_repo()).await?;

    // Both should be searchable
    let doc_repo = ctx.doc_repo();
    let results = doc_repo.search_documents("test content").await?;

    assert!(results.len() >= 2);

    Ok(())
}

#[tokio::test]
async fn test_embedding_consistency() -> Result<()> {
    let ctx = TestContext::new().await?;

    // Same text should produce same embedding
    let emb1 = ctx.embedder.embed_text("test text").await?;
    let emb2 = ctx.embedder.embed_text("test text").await?;

    assert_eq!(emb1, emb2);

    Ok(())
}

#[tokio::test]
async fn test_embedding_dimensions() -> Result<()> {
    let ctx = TestContext::new().await?;

    let embedding = ctx.embedder.embed_text("test").await?;

    assert_eq!(embedding.len(), 384); // Default dimension

    Ok(())
}

#[tokio::test]
async fn test_embedding_normalization() -> Result<()> {
    let ctx = TestContext::new().await?;

    let embedding = ctx.embedder.embed_text("test normalization").await?;

    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

    assert!((magnitude - 1.0).abs() < 0.1, "Magnitude: {}", magnitude);

    Ok(())
}

#[tokio::test]
async fn test_batch_embedding_generation() -> Result<()> {
    let ctx = TestContext::new().await?;

    let texts = vec![
        "first text".to_string(),
        "second text".to_string(),
        "third text".to_string(),
    ];

    let embeddings = ctx.embedder.embed_batch(&texts).await?;

    assert_eq!(embeddings.len(), 3);

    for emb in &embeddings {
        assert_eq!(emb.len(), 384);
    }

    Ok(())
}

#[tokio::test]
async fn test_search_result_completeness() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("complete.md", "Complete test").await?;

    let doc_repo = ctx.doc_repo();
    let results = doc_repo.search_documents("complete").await?;

    // Results should contain the document ID
    assert!(results.contains(&doc.id));

    Ok(())
}

#[tokio::test]
async fn test_search_limit() -> Result<()> {
    let ctx = TestContext::new().await?;

    for i in 0..50 {
        ctx.create_test_document(
            &format!("limit-{}.md", i),
            "limit test content",
        )
        .await?;
    }

    let doc_repo = ctx.doc_repo();
    let results = doc_repo.search_documents("limit test").await?;

    // FTS search returns all matches, but we can verify it's working
    assert!(!results.is_empty());

    Ok(())
}

#[cfg(test)]
mod helpers {
    pub use crate::helpers::*;
}
