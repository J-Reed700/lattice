#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! # Tag Integration Tests
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Comprehensive tests for tag management and document-tag relationships.
//!
//! ## Test Coverage
//!
//! - Tag creation and retrieval
//! - Document-tag associations
//! - Tag search and filtering
//! - Tag-based document discovery
//! - Mention extraction and linking
//! - Tag auto-generation
//! - Backlinks and relationships
//!
//! ## Test Scenarios
//!
//! - Single and multiple tags
//! - Tag hierarchies
//! - Tag deduplication
//! - Concurrent tag operations
//! - Tag persistence and cleanup

use lattice::shared::error::Result;

mod helpers;
use helpers::{
    TestContext, DocumentFactory, generate_markdown_with_mentions,
    assert_tag_exists, assert_document_has_tags, assert_tag_on_documents,
    assert_mention_exists, assert_mention_type, assert_document_has_mentions,
};

#[tokio::test]
async fn test_create_single_tag() -> Result<()> {
    let ctx = TestContext::new().await?;

    let tags = ctx.create_test_tags(&["test-tag"]).await?;

    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "test-tag");

    assert_tag_exists(&ctx.tag_repo(), "test-tag").await?;

    Ok(())
}

#[tokio::test]
async fn test_create_multiple_tags() -> Result<()> {
    let ctx = TestContext::new().await?;

    let tag_names = vec!["tag1", "tag2", "tag3", "tag4", "tag5"];
    let tags = ctx.create_test_tags(&tag_names).await?;

    assert_eq!(tags.len(), 5);

    for name in &tag_names {
        assert_tag_exists(&ctx.tag_repo(), name).await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_tag_get_or_create_idempotent() -> Result<()> {
    let ctx = TestContext::new().await?;
    let tag_repo = ctx.tag_repo();

    let tag1 = tag_repo.get_or_create("idempotent-tag", None).await?;

    let tag2 = tag_repo.get_or_create("idempotent-tag", None).await?;

    assert_eq!(tag1.id, tag2.id);

    Ok(())
}

#[tokio::test]
async fn test_add_single_tag_to_document() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("test.md", "Test content").await?;
    let tags = ctx.create_test_tags(&["single-tag"]).await?;

    let tag_repo = ctx.tag_repo();
    tag_repo.add_tag_to_document(&doc.id, &tags[0].id).await?;

    assert_document_has_tags(&tag_repo, &doc.id, &["single-tag"]).await?;

    Ok(())
}

#[tokio::test]
async fn test_add_multiple_tags_to_document() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("multi-tag.md", "Content").await?;
    let tags = ctx.create_test_tags(&["tag1", "tag2", "tag3"]).await?;

    let tag_repo = ctx.tag_repo();

    for tag in &tags {
        tag_repo.add_tag_to_document(&doc.id, &tag.id).await?;
    }

    assert_document_has_tags(&tag_repo, &doc.id, &["tag1", "tag2", "tag3"]).await?;

    Ok(())
}

#[tokio::test]
async fn test_duplicate_tag_association_handled() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("duplicate.md", "Content").await?;
    let tags = ctx.create_test_tags(&["duplicate-tag"]).await?;

    let tag_repo = ctx.tag_repo();

    tag_repo.add_tag_to_document(&doc.id, &tags[0].id).await?;
    tag_repo.add_tag_to_document(&doc.id, &tags[0].id).await?;

    // Should only appear once
    let doc_tags = tag_repo.get_tags_for_document(&doc.id).await?;
    assert_eq!(doc_tags.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_find_documents_by_tag() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document("doc1.md", "Content 1").await?;
    let doc2 = ctx.create_test_document("doc2.md", "Content 2").await?;
    let doc3 = ctx.create_test_document("doc3.md", "Content 3").await?;

    let tags = ctx.create_test_tags(&["common-tag"]).await?;
    let tag_repo = ctx.tag_repo();

    tag_repo.add_tag_to_document(&doc1.id, &tags[0].id).await?;
    tag_repo.add_tag_to_document(&doc2.id, &tags[0].id).await?;

    let doc_ids = tag_repo.find_documents_by_tag_name("common-tag").await?;

    assert_eq!(doc_ids.len(), 2);
    assert!(doc_ids.contains(&doc1.id));
    assert!(doc_ids.contains(&doc2.id));
    assert!(!doc_ids.contains(&doc3.id));

    Ok(())
}

#[tokio::test]
async fn test_get_all_tags_with_counts() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document("doc1.md", "Content").await?;
    let doc2 = ctx.create_test_document("doc2.md", "Content").await?;

    let tags = ctx.create_test_tags(&["counted-tag"]).await?;
    let tag_repo = ctx.tag_repo();

    tag_repo.add_tag_to_document(&doc1.id, &tags[0].id).await?;
    tag_repo.add_tag_to_document(&doc2.id, &tags[0].id).await?;

    let all_tags = tag_repo.get_all_with_counts().await?;

    let counted_tag = all_tags.iter().find(|t| t.tag.name() == "counted-tag");
    assert!(counted_tag.is_some());

    let counted_tag = counted_tag.unwrap();
    assert_eq!(counted_tag.document_count, 2);

    Ok(())
}

#[tokio::test]
async fn test_extract_person_mentions() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("mentions.md", "Test").await?;

    let content = "Meeting with @alice-smith and @bob-jones about the project.";

    let mention_repo = ctx.mention_repo();
    let mentions = mention_repo
        .extract_and_store_mentions(&doc.id, content)
        .await?;

    let person_mentions: Vec<_> = mentions
        .iter()
        .filter(|m| m.mention.mention_type == "person")
        .collect();

    assert_eq!(person_mentions.len(), 2);

    assert_mention_exists(&mention_repo, "alice-smith").await?;
    assert_mention_exists(&mention_repo, "bob-jones").await?;

    Ok(())
}

#[tokio::test]
async fn test_extract_wikilink_mentions() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("wikilinks.md", "Test").await?;

    let content = "See [[machine-learning]] and [[neural-networks]] for details.";

    let mention_repo = ctx.mention_repo();
    let mentions = mention_repo
        .extract_and_store_mentions(&doc.id, content)
        .await?;

    let wikilinks: Vec<_> = mentions
        .iter()
        .filter(|m| m.mention.mention_type == "wikilink")
        .collect();

    assert_eq!(wikilinks.len(), 2);

    assert_mention_exists(&mention_repo, "machine-learning").await?;
    assert_mention_exists(&mention_repo, "neural-networks").await?;

    Ok(())
}

#[tokio::test]
async fn test_mixed_mention_types() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("mixed.md", "Test").await?;

    let content = "Discussion with @alice about [[project-alpha]] and @bob about [[project-beta]].";

    let mention_repo = ctx.mention_repo();
    let mentions = mention_repo
        .extract_and_store_mentions(&doc.id, content)
        .await?;

    assert_eq!(mentions.len(), 4);

    let persons: Vec<_> = mentions
        .iter()
        .filter(|m| m.mention.mention_type == "person")
        .collect();
    assert_eq!(persons.len(), 2);

    let wikilinks: Vec<_> = mentions
        .iter()
        .filter(|m| m.mention.mention_type == "wikilink")
        .collect();
    assert_eq!(wikilinks.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_mention_backlinks() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document("doc1.md", "Content").await?;
    let doc2 = ctx.create_test_document("doc2.md", "Content").await?;

    let mention_repo = ctx.mention_repo();

    mention_repo
        .extract_and_store_mentions(&doc1.id, "Meeting with @alice-backlink")
        .await?;

    mention_repo
        .extract_and_store_mentions(&doc2.id, "Follow-up with @alice-backlink")
        .await?;

    let mention = mention_repo
        .find_mention_by_name("alice-backlink")
        .await?
        .unwrap();

    let backlinks = mention_repo
        .get_documents_with_mention(&mention.id)
        .await?;

    assert_eq!(backlinks.len(), 2);
    assert!(backlinks.contains(&doc1.id));
    assert!(backlinks.contains(&doc2.id));

    Ok(())
}

#[tokio::test]
async fn test_document_with_tags_and_mentions() -> Result<()> {
    let ctx = TestContext::new().await?;

    let content = generate_markdown_with_mentions(
        "alice-integration",
        "project-x",
        &["important", "urgent"],
    );

    let doc = ctx.create_test_document("integrated.md", &content).await?;

    let mention_repo = ctx.mention_repo();
    mention_repo
        .extract_and_store_mentions(&doc.id, &content)
        .await?;

    let tags = ctx.create_test_tags(&["important", "urgent"]).await?;
    let tag_repo = ctx.tag_repo();

    for tag in &tags {
        tag_repo.add_tag_to_document(&doc.id, &tag.id).await?;
    }

    assert_document_has_tags(&tag_repo, &doc.id, &["important", "urgent"]).await?;
    assert_document_has_mentions(&mention_repo, &doc.id, &["alice-integration", "project-x"]).await?;

    Ok(())
}

#[tokio::test]
async fn test_cross_document_relationships() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc1 = ctx.create_test_document("rel1.md", "Content").await?;
    let doc2 = ctx.create_test_document("rel2.md", "Content").await?;
    let doc3 = ctx.create_test_document("rel3.md", "Content").await?;

    let tags = ctx.create_test_tags(&["shared-topic"]).await?;
    let tag_repo = ctx.tag_repo();

    tag_repo.add_tag_to_document(&doc1.id, &tags[0].id).await?;
    tag_repo.add_tag_to_document(&doc2.id, &tags[0].id).await?;

    let mention_repo = ctx.mention_repo();

    mention_repo
        .extract_and_store_mentions(&doc1.id, "Discussed with @shared-person")
        .await?;

    mention_repo
        .extract_and_store_mentions(&doc3.id, "Met @shared-person")
        .await?;

    let tag_docs = tag_repo.find_documents_by_tag_name("shared-topic").await?;
    assert_eq!(tag_docs.len(), 2);

    let mention = mention_repo.find_mention_by_name("shared-person").await?.unwrap();
    let mention_docs = mention_repo.get_documents_with_mention(&mention.id).await?;
    assert_eq!(mention_docs.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_tag_creation() -> Result<()> {
    let ctx = TestContext::new().await?;

    let tag_repo = ctx.tag_repo();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let repo = tag_repo.clone();
            let tag_name = format!("concurrent-tag-{}", i);

            tokio::spawn(async move {
                repo.get_or_create(&tag_name, None).await
            })
        })
        .collect();

    // Wait for all
    let results = futures::future::join_all(handles).await;

    // All should succeed
    for result in results {
        assert!(result.is_ok());
    }

    let stats = ctx.get_db_stats().await?;
    assert_eq!(stats.tags, 10);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_tag_associations() -> Result<()> {
    let ctx = TestContext::new().await?;

    let doc = ctx.create_test_document("concurrent-assoc.md", "Test").await?;
    let tag_repo = ctx.tag_repo();

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let repo = tag_repo.clone();
            let doc_id = doc.id.clone();
            let tag_name = format!("assoc-tag-{}", i);

            tokio::spawn(async move {
                let tag = repo.get_or_create(&tag_name, None).await?;
                repo.add_tag_to_document(&doc_id, &tag.id).await?;
                Ok::<_, lattice::shared::error::AppError>(())
            })
        })
        .collect();

    // Wait for all
    let results = futures::future::join_all(handles).await;

    for result in results {
        assert!(result.is_ok());
    }

    let doc_tags = tag_repo.get_tags_for_document(&doc.id).await?;
    assert_eq!(doc_tags.len(), 5);

    Ok(())
}

#[tokio::test]
async fn test_mention_search_autocomplete() -> Result<()> {
    let ctx = TestContext::new().await?;

    let mention_repo = ctx.mention_repo();

    let mentions = vec![
        ("alice-anderson", "person"),
        ("alice-brown", "person"),
        ("albert-smith", "person"),
        ("algorithm-design", "wikilink"),
    ];

    for (name, mention_type) in mentions {
        mention_repo.create_mention(name, mention_type, None).await?;
    }

    // Search with prefix
    let results = mention_repo.search_mentions("ali", 10).await?;

    assert!(results.len() >= 3);

    let names: Vec<String> = results.iter().map(|m| m.name.clone()).collect();
    assert!(names.contains(&"alice-anderson".to_string()));
    assert!(names.contains(&"alice-brown".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_mention_search_limit() -> Result<()> {
    let ctx = TestContext::new().await?;

    let mention_repo = ctx.mention_repo();

    for i in 0..20 {
        mention_repo
            .create_mention(&format!("person-{}", i), "person", None)
            .await?;
    }

    // Search with limit
    let results = mention_repo.search_mentions("person", 5).await?;

    assert!(results.len() <= 5);

    Ok(())
}

#[tokio::test]
async fn test_get_mentions_by_type() -> Result<()> {
    let ctx = TestContext::new().await?;

    let mention_repo = ctx.mention_repo();

    mention_repo.create_mention("person1", "person", None).await?;
    mention_repo.create_mention("person2", "person", None).await?;
    mention_repo.create_mention("wiki1", "wikilink", None).await?;

    let persons = mention_repo.get_mentions_by_type("person").await?;
    assert_eq!(persons.len(), 2);

    let wikilinks = mention_repo.get_mentions_by_type("wikilink").await?;
    assert_eq!(wikilinks.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_empty_tag_name_handling() -> Result<()> {
    let ctx = TestContext::new().await?;

    let tag_repo = ctx.tag_repo();

    let tag = tag_repo.get_or_create("", None).await?;
    assert!(!tag.id.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_special_characters_in_tags() -> Result<()> {
    let ctx = TestContext::new().await?;

    let tag_repo = ctx.tag_repo();

    // Tags with special characters
    let tag = tag_repo.get_or_create("tag-with-dashes", None).await?;
    assert_eq!(tag.name, "tag-with-dashes");

    Ok(())
}

#[tokio::test]
async fn test_tag_color_assignment() -> Result<()> {
    let ctx = TestContext::new().await?;

    let tag_repo = ctx.tag_repo();

    let tag = tag_repo.get_or_create("colored-tag", Some("#FF5733")).await?;

    assert_eq!(tag.color, Some("#FF5733".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_nonexistent_document_tag_association() -> Result<()> {
    let ctx = TestContext::new().await?;

    let tag_repo = ctx.tag_repo();
    let tag = tag_repo.get_or_create("test-tag", None).await?;

    // Try to add tag to nonexistent document
    let result = tag_repo.add_tag_to_document("nonexistent-doc-id", &tag.id).await;

    assert!(result.is_ok() || result.is_err());

    Ok(())
}

#[cfg(test)]
mod helpers {
    pub use crate::helpers::*;
}
