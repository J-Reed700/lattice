#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! DISABLED: Tag generator integration tests

//!
//! These tests are temporarily disabled because the TagGenerator module has been
//! disabled as part of the local-first refactoring (removing cloud provider dependencies).
//!
//! TODO: Re-enable these tests once TagGenerator is re-implemented using local LLM (Ollama).
//!       See: src/src/src/infrastructure/extraction/tag_generator.rs
//!
//! Original purpose: Tests LLM-based tag generation with Anthropic Claude.

// DISABLED: Waiting for local LLM implementation
// use lattice::extraction::tag_generator::{TagGenerator, DocumentMetadata};

/*
DISABLED: All tests below are commented out pending local LLM implementation.

#[test]
fn test_tag_generator_creation() {
    let result = TagGenerator::new("test-api-key");
    assert!(result.is_ok());
}

#[test]
fn test_tag_generator_creation_empty_key() {
    let result = TagGenerator::new("");
    assert!(result.is_err());
}

#[test]
fn test_document_metadata_serialization() {
    let metadata = DocumentMetadata {
        title: Some("Test Document".to_string()),
        file_type: Some("md".to_string()),
        author: Some("Test Author".to_string()),
    };

    let json = serde_json::to_string(&metadata).unwrap();
    assert!(json.contains("Test Document"));
    assert!(json.contains("Test Author"));
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_generate_tags_simple() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY environment variable required");

    let generator = TagGenerator::new(&api_key)
        .expect("Failed to create generator");

    let content = "This document is about machine learning and artificial intelligence. \
                   It covers neural networks, deep learning, and natural language processing.";

    let tags = generator.generate_tags(content, None, 5).await;

    assert!(tags.is_ok(), "Tag generation failed: {:?}", tags.err());

    let tags = tags.unwrap();
    assert!(!tags.is_empty(), "No tags generated");
    assert!(tags.len() <= 5, "Too many tags generated");

    println!("Generated tags: {:?}", tags);

    let tags_lower: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
    let is_relevant = tags_lower.iter().any(|t| {
        t.contains("machine") ||
        t.contains("learning") ||
        t.contains("ai") ||
        t.contains("neural") ||
        t.contains("deep")
    });

    assert!(is_relevant, "Tags don't seem relevant to content: {:?}", tags);
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_generate_tags_with_metadata() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    let content = "This is a tutorial about Python programming.";

    let metadata = Some(DocumentMetadata {
        title: Some("Python Tutorial".to_string()),
        file_type: Some("md".to_string()),
        author: None,
    });

    let tags = generator.generate_tags(content, metadata, 5).await;

    assert!(tags.is_ok());
    let tags = tags.unwrap();

    println!("Tags with metadata: {:?}", tags);

    let tags_lower: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
    let has_relevant = tags_lower.iter().any(|t| {
        t.contains("python") ||
        t.contains("programming") ||
        t.contains("tutorial") ||
        t.contains("code")
    });

    assert!(has_relevant, "Tags not relevant: {:?}", tags);
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_generate_tags_max_limit() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    let content = "A very long document about many topics: science, technology, art, \
                   music, literature, history, geography, mathematics, physics, chemistry.";

    // Request only 3 tags
    let tags = generator.generate_tags(content, None, 3).await;

    assert!(tags.is_ok());
    let tags = tags.unwrap();

    println!("Limited tags (max 3): {:?}", tags);

    assert!(tags.len() <= 3, "Generated more tags than requested: {}", tags.len());
    assert!(!tags.is_empty(), "No tags generated");
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_batch_tag_generation() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    let documents = vec![
        (
            "This is about machine learning and neural networks.".to_string(),
            Some(DocumentMetadata {
                title: Some("ML Guide".to_string()),
                file_type: Some("md".to_string()),
                author: None,
            }),
        ),
        (
            "This is a cooking recipe for chocolate cake.".to_string(),
            Some(DocumentMetadata {
                title: Some("Chocolate Cake Recipe".to_string()),
                file_type: Some("md".to_string()),
                author: None,
            }),
        ),
        (
            "This is about web development with React and TypeScript.".to_string(),
            None,
        ),
    ];

    let results = generator.generate_tags_batch(documents, 5).await;

    assert!(results.is_ok(), "Batch generation failed: {:?}", results.err());

    let results = results.unwrap();
    assert_eq!(results.len(), 3, "Should generate tags for all 3 documents");

    println!("Batch results:");
    for (i, tags) in results.iter().enumerate() {
        println!("  Document {}: {:?}", i, tags);
        assert!(!tags.is_empty(), "Document {} has no tags", i);
    }

    let tags1_lower: Vec<String> = results[0].iter().map(|t| t.to_lowercase()).collect();
    let tags2_lower: Vec<String> = results[1].iter().map(|t| t.to_lowercase()).collect();

    let ml_relevant = tags1_lower.iter().any(|t| {
        t.contains("machine") || t.contains("learning") || t.contains("neural")
    });
    let cooking_relevant = tags2_lower.iter().any(|t| {
        t.contains("cook") || t.contains("recipe") || t.contains("cake") || t.contains("food")
    });

    assert!(ml_relevant, "First doc tags not ML-related: {:?}", results[0]);
    assert!(cooking_relevant, "Second doc tags not cooking-related: {:?}", results[1]);
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_prompt_caching_effectiveness() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    let documents: Vec<(String, Option<DocumentMetadata>)> = (0..5)
        .map(|i| {
            (
                format!("This is test document number {} about software development.", i),
                Some(DocumentMetadata {
                    title: Some(format!("Doc {}", i)),
                    file_type: Some("md".to_string()),
                    author: None,
                }),
            )
        })
        .collect();

    let _ = generator.generate_tags_batch(documents, 5).await;

    let stats = generator.get_cache_stats();
    println!("Cache stats after 5 documents: {:?}", stats);

    // After first document, subsequent ones should hit cache
    println!("Hits: {}, Misses: {}, Hit rate: {:.1}%",
             stats.hits, stats.misses, stats.hit_rate_percent());

    // We expect at least some cache hits
    if stats.hits > 0 {
        println!("✓ Prompt caching is working!");
        println!("  Tokens saved: {}", stats.total_tokens_saved);
        assert!(stats.total_tokens_saved > 0, "No tokens saved despite cache hits");
    } else {
        println!("⚠ No cache hits (might be too soon or API issue)");
    }
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_cache_stats_tracking() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    // Initial stats should be zero
    let initial_stats = generator.get_cache_stats();
    assert_eq!(initial_stats.hits, 0);
    assert_eq!(initial_stats.misses, 0);
    assert_eq!(initial_stats.total_tokens_saved, 0);
    assert_eq!(initial_stats.hit_rate_percent(), 0.0);

    let _ = generator.generate_tags("Test content", None, 3).await;

    let after_stats = generator.get_cache_stats();
    println!("Stats after one generation: {:?}", after_stats);

    assert!(after_stats.hits + after_stats.misses > 0);
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_empty_content_handling() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    let tags = generator.generate_tags("", None, 5).await;

    match tags {
        Ok(tags) => {
            println!("Tags for empty content: {:?}", tags);
            // Empty content might return no tags or generic ones
        }
        Err(e) => {
            println!("Empty content error (expected): {}", e);
            // This is also acceptable
        }
    }
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_very_long_content() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    let content = "This is about machine learning. ".repeat(500);

    let tags = generator.generate_tags(&content, None, 5).await;

    assert!(tags.is_ok(), "Failed to handle long content: {:?}", tags.err());

    let tags = tags.unwrap();
    println!("Tags for long content: {:?}", tags);

    assert!(!tags.is_empty());
    assert!(tags.len() <= 5);
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_special_characters_in_content() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    let content = r#"This document has "quotes", 'apostrophes', and special chars: @#$%^&*()
    It also has code: `function test() { return true; }`
    And URLs: https://example.com
    And emojis: 🚀 🎯 ✨
    "#;

    let tags = generator.generate_tags(content, None, 5).await;

    assert!(tags.is_ok(), "Failed with special characters: {:?}", tags.err());

    let tags = tags.unwrap();
    println!("Tags with special chars: {:?}", tags);

    assert!(!tags.is_empty());
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_non_english_content() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    let content = "Este es un documento sobre inteligencia artificial y aprendizaje automático.";

    let tags = generator.generate_tags(content, None, 5).await;

    assert!(tags.is_ok(), "Failed with non-English content: {:?}", tags.err());

    let tags = tags.unwrap();
    println!("Tags for Spanish content: {:?}", tags);

    assert!(!tags.is_empty());
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_concurrent_tag_generation() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY required");

    let generator = TagGenerator::new(&api_key).unwrap();

    let handles: Vec<_> = (0..3)
        .map(|i| {
            let gen_clone = generator.clone();
            tokio::spawn(async move {
                let content = format!("Document {} about programming", i);
                gen_clone.generate_tags(&content, None, 3).await
            })
        })
        .collect();

    let results = futures::future::join_all(handles).await;

    // All should succeed
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "Concurrent request {} failed", i);
        let tags = result.as_ref().unwrap();
        assert!(tags.is_ok(), "Tag generation {} failed", i);
    }
}

#[test]
fn test_tag_normalization() {
    // Test that tags are normalized properly
    // This would be a unit test for the internal normalization function
    // if exposed, or we can test via the API

    // Tags should be lowercase, hyphenated, etc.
    // E.g., "Machine Learning" -> "machine-learning"

    // This test would need access to internal normalization function
    // For now, we can verify through the API results
}

#[tokio::test]
#[ignore] // Requires API key
async fn test_error_handling_invalid_api_key() {
    let generator = TagGenerator::new("invalid-key-sk-ant-test");

    if let Ok(gen) = generator {
        let result = gen.generate_tags("Test content", None, 3).await;

        assert!(result.is_err(), "Should fail with invalid API key");

        let error = result.unwrap_err();
        println!("Expected error: {}", error);

        // Error should mention authentication or API key
        let error_str = error.to_string().to_lowercase();
        assert!(
            error_str.contains("auth") ||
            error_str.contains("api") ||
            error_str.contains("key") ||
            error_str.contains("401"),
            "Error should mention authentication: {}", error
        );
    }
}

End of disabled tests - waiting for local LLM implementation.
*/
