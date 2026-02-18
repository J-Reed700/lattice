#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! # Test Assertions
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Custom assertion helpers for integration tests.
//!
//! ## Purpose
//!
//! Provides domain-specific assertions that:
//! - Make tests more readable
//! - Provide better error messages
//! - Reduce code duplication
//!
//! ## Examples
//!
//! ```rust
//! assert_embeddings_similar(&emb1, &emb2, 0.9);
//! assert_document_exists(&repo, "doc-123").await?;
//! assert_chunk_count(&repo, "doc-123", 5).await?;
//! ```

use vault::error::Result;
use vault::infrastructure::persistence::repositories::{
    chunk_repository::ChunkRepository,
    document_repository::DocumentRepository,
    embedding_repository::EmbeddingRepository,
    mention_repository::MentionRepository,
    tag_repository::TagRepository,
};

// ============================================================================
// Vector/Embedding Assertions
// ============================================================================

/// Assert that two embeddings are similar within a threshold.
///
/// # Arguments
///
/// * `embedding1` - First embedding
/// * `embedding2` - Second embedding
/// * `threshold` - Minimum cosine similarity (0.0 to 1.0)
///
/// # Panics
///
/// Panics if similarity is below threshold
pub fn assert_embeddings_similar(embedding1: &[f32], embedding2: &[f32], threshold: f32) {
    let similarity = cosine_similarity(embedding1, embedding2);

    assert!(
        similarity >= threshold,
        "Embeddings not similar enough: {} < {} (threshold)",
        similarity,
        threshold
    );
}

/// Assert that two embeddings are different (below threshold).
pub fn assert_embeddings_different(embedding1: &[f32], embedding2: &[f32], threshold: f32) {
    let similarity = cosine_similarity(embedding1, embedding2);

    assert!(
        similarity < threshold,
        "Embeddings too similar: {} >= {} (threshold)",
        similarity,
        threshold
    );
}

/// Assert embedding has correct dimensions.
pub fn assert_embedding_dimensions(embedding: &[f32], expected: usize) {
    assert_eq!(
        embedding.len(),
        expected,
        "Embedding has wrong dimensions: {} != {}",
        embedding.len(),
        expected
    );
}

/// Assert embedding is normalized (magnitude ~= 1.0).
pub fn assert_embedding_normalized(embedding: &[f32]) {
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

    assert!(
        (magnitude - 1.0).abs() < 0.01,
        "Embedding not normalized: magnitude = {}",
        magnitude
    );
}

// ============================================================================
// Document Assertions
// ============================================================================

/// Assert that a document exists in the database.
///
/// # Arguments
///
/// * `repo` - Document repository
/// * `doc_id` - Document ID to check
///
/// # Panics
///
/// Panics if document does not exist
pub async fn assert_document_exists(repo: &DocumentRepository, doc_id: &str) -> Result<()> {
    use vault::application::ports::repository_port::RepositoryPort;

    let doc = repo.find_by_id(doc_id).await?;

    assert!(
        doc.is_some(),
        "Document not found: {}",
        doc_id
    );

    Ok(())
}

/// Assert that a document does not exist.
pub async fn assert_document_not_exists(repo: &DocumentRepository, doc_id: &str) -> Result<()> {
    use vault::application::ports::repository_port::RepositoryPort;

    let doc = repo.find_by_id(doc_id).await?;

    assert!(
        doc.is_none(),
        "Document should not exist: {}",
        doc_id
    );

    Ok(())
}

/// Assert document has expected number of chunks.
pub async fn assert_chunk_count(repo: &ChunkRepository, doc_id: &str, expected: usize) -> Result<()> {
    use vault::application::ports::chunk_repository_port::ChunkRepositoryPort;

    let count = repo.count_by_document(doc_id).await?;

    assert_eq!(
        count,
        expected,
        "Wrong number of chunks for document {}: {} != {}",
        doc_id,
        count,
        expected
    );

    Ok(())
}

// ============================================================================
// Tag Assertions
// ============================================================================

/// Assert that a tag exists.
pub async fn assert_tag_exists(repo: &TagRepository, tag_name: &str) -> Result<()> {
    let tags = repo.get_all_with_counts().await?;
    let found = tags.iter().any(|(tag, _count)| tag.name().as_str() == tag_name);

    assert!(
        found,
        "Tag not found: {}",
        tag_name
    );

    Ok(())
}

/// Assert document has specific tags.
pub async fn assert_document_has_tags(
    repo: &TagRepository,
    doc_id: &str,
    expected_tags: &[&str],
) -> Result<()> {
    let tags = repo.get_tags_for_document(doc_id).await?;
    let tag_names: Vec<String> = tags.iter().map(|t| t.name().as_str().to_string()).collect();

    for expected in expected_tags {
        assert!(
            tag_names.contains(&expected.to_string()),
            "Document {} missing tag: {}",
            doc_id,
            expected
        );
    }

    Ok(())
}

/// Assert tag is associated with specific documents.
pub async fn assert_tag_on_documents(
    repo: &TagRepository,
    tag_name: &str,
    expected_doc_ids: &[&str],
) -> Result<()> {
    let doc_ids = repo.find_documents_by_tag_name(tag_name).await?;

    for expected in expected_doc_ids {
        assert!(
            doc_ids.contains(&expected.to_string()),
            "Tag {} not on document: {}",
            tag_name,
            expected
        );
    }

    Ok(())
}

// ============================================================================
// Mention Assertions
// ============================================================================

/// Assert that a mention exists.
pub async fn assert_mention_exists(repo: &MentionRepository, mention_name: &str) -> Result<()> {
    use vault::application::ports::mention_repository_port::MentionRepositoryPort;

    let mention = repo.find_mention_by_name(mention_name).await?;

    assert!(
        mention.is_some(),
        "Mention not found: {}",
        mention_name
    );

    Ok(())
}

/// Assert mention has correct type.
pub async fn assert_mention_type(
    repo: &MentionRepository,
    mention_name: &str,
    expected_type: &str,
) -> Result<()> {
    use vault::application::ports::mention_repository_port::MentionRepositoryPort;

    let mention = repo.find_mention_by_name(mention_name).await?;

    assert!(
        mention.is_some(),
        "Mention not found: {}",
        mention_name
    );

    let mention = mention.unwrap();
    assert_eq!(
        mention.mention_type,
        expected_type,
        "Wrong mention type for {}: {} != {}",
        mention_name,
        mention.mention_type,
        expected_type
    );

    Ok(())
}

/// Assert document contains specific mentions.
pub async fn assert_document_has_mentions(
    repo: &MentionRepository,
    doc_id: &str,
    expected_mentions: &[&str],
) -> Result<()> {
    use vault::application::ports::mention_repository_port::MentionRepositoryPort;

    for mention_name in expected_mentions {
        let mention = repo.find_mention_by_name(mention_name).await?;

        assert!(
            mention.is_some(),
            "Mention not found: {}",
            mention_name
        );

        let mention = mention.unwrap();
        let docs = repo.get_documents_with_mention(&mention.id).await?;

        assert!(
            docs.contains(&doc_id.to_string()),
            "Document {} missing mention: {}",
            doc_id,
            mention_name
        );
    }

    Ok(())
}

// ============================================================================
// Search Result Assertions
// ============================================================================

/// Assert search results contain specific document IDs.
pub fn assert_search_contains(results: &[String], expected_doc_ids: &[&str]) {
    for expected in expected_doc_ids {
        assert!(
            results.contains(&expected.to_string()),
            "Search results missing document: {}",
            expected
        );
    }
}

/// Assert search results are in order (by ID).
pub fn assert_search_order(results: &[String], expected_order: &[&str]) {
    assert_eq!(
        results.len(),
        expected_order.len(),
        "Wrong number of search results: {} != {}",
        results.len(),
        expected_order.len()
    );

    for (i, expected) in expected_order.iter().enumerate() {
        assert_eq!(
            &results[i],
            expected,
            "Wrong result at position {}: {} != {}",
            i,
            results[i],
            expected
        );
    }
}

/// Assert minimum number of search results.
pub fn assert_min_results(results: &[String], min: usize) {
    assert!(
        results.len() >= min,
        "Too few search results: {} < {}",
        results.len(),
        min
    );
}

/// Assert maximum number of search results.
pub fn assert_max_results(results: &[String], max: usize) {
    assert!(
        results.len() <= max,
        "Too many search results: {} > {}",
        results.len(),
        max
    );
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Calculate cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

// ============================================================================
// Batch Assertions
// ============================================================================

/// Assert multiple documents exist.
pub async fn assert_documents_exist(repo: &DocumentRepository, doc_ids: &[&str]) -> Result<()> {
    for doc_id in doc_ids {
        assert_document_exists(repo, doc_id).await?;
    }
    Ok(())
}

/// Assert multiple tags exist.
pub async fn assert_tags_exist(repo: &TagRepository, tag_names: &[&str]) -> Result<()> {
    for tag_name in tag_names {
        assert_tag_exists(repo, tag_name).await?;
    }
    Ok(())
}

/// Assert multiple mentions exist.
pub async fn assert_mentions_exist(repo: &MentionRepository, mention_names: &[&str]) -> Result<()> {
    for mention_name in mention_names {
        assert_mention_exists(repo, mention_name).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.0001);
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.0001);
    }

    #[test]
    fn test_assert_embedding_dimensions() {
        let emb = vec![0.0; 384];
        assert_embedding_dimensions(&emb, 384);
    }

    #[test]
    #[should_panic(expected = "Embedding has wrong dimensions")]
    fn test_assert_embedding_dimensions_fails() {
        let emb = vec![0.0; 128];
        assert_embedding_dimensions(&emb, 384);
    }

    #[test]
    fn test_assert_embedding_normalized() {
        let emb = vec![0.6, 0.8]; // 3-4-5 triangle, normalized
        assert_embedding_normalized(&emb);
    }

    #[test]
    fn test_assert_embeddings_similar() {
        let emb1 = vec![1.0, 0.0, 0.0];
        let emb2 = vec![0.99, 0.01, 0.0];

        assert_embeddings_similar(&emb1, &emb2, 0.9);
    }

    #[test]
    #[should_panic(expected = "Embeddings not similar enough")]
    fn test_assert_embeddings_similar_fails() {
        let emb1 = vec![1.0, 0.0, 0.0];
        let emb2 = vec![0.0, 1.0, 0.0];

        assert_embeddings_similar(&emb1, &emb2, 0.9);
    }

    #[test]
    fn test_assert_search_contains() {
        let results = vec!["doc1".to_string(), "doc2".to_string(), "doc3".to_string()];
        assert_search_contains(&results, &["doc1", "doc3"]);
    }

    #[test]
    fn test_assert_min_max_results() {
        let results = vec!["doc1".to_string(), "doc2".to_string()];
        assert_min_results(&results, 1);
        assert_max_results(&results, 5);
    }
}
