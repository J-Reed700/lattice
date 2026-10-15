use super::*;
use lattice_core::application::contracts::web::CleanArticle;

#[test]
fn test_extract_domain() {
    assert_eq!(
        WebArchiveService::extract_domain("https://github.com/user/repo").unwrap(),
        "github.com"
    );
    assert_eq!(
        WebArchiveService::extract_domain("https://www.example.com/article").unwrap(),
        "www.example.com"
    );
    assert_eq!(
        WebArchiveService::extract_domain("http://arxiv.org/abs/1234").unwrap(),
        "arxiv.org"
    );
}

#[test]
fn test_slugify() {
    assert_eq!(
        WebArchiveService::slugify("Rust Best Practices"),
        "rust-best-practices"
    );
    assert_eq!(
        WebArchiveService::slugify("Hello, World! 123"),
        "hello-world-123"
    );
    assert_eq!(
        WebArchiveService::slugify("Multiple   Spaces"),
        "multiple-spaces"
    );

    let long_title = "a".repeat(100);
    assert_eq!(WebArchiveService::slugify(&long_title).len(), 50);
}

#[test]
fn test_article_to_markdown() {
    let article = CleanArticle {
        title: "Test Article".to_string(),
        author: Some("Test Author".to_string()),
        content: "<p>Content</p>".to_string(),
        text_content: "Content here".to_string(),
        word_count: 2,
        reading_time_minutes: 1,
        published_date: None,
        excerpt: Some("Brief excerpt".to_string()),
    };

    let markdown = WebArchiveService::article_to_markdown(&article, "https://example.com");

    assert!(markdown.contains("---"));
    assert!(markdown.contains("title: \"Test Article\""));
    assert!(markdown.contains("author: \"Test Author\""));
    assert!(markdown.contains("url: \"https://example.com\""));
    assert!(markdown.contains("word_count: 2"));
    assert!(markdown.contains("# Test Article"));
    assert!(markdown.contains("*By Test Author*"));
    assert!(markdown.contains("> Brief excerpt"));
    assert!(markdown.contains("Content here"));
}

#[tokio::test]
async fn test_markdown_to_article() {
    let markdown = r#"---
title: "Test Article"
author: "Test Author"
url: "https://example.com"
word_count: 150
reading_time_minutes: 1
archived_at: "2024-01-20T14:22:00Z"
---

*By Test Author*

> Brief excerpt

Article content here.
"#;

    let article = WebArchiveService::markdown_to_article(markdown)
        .await
        .unwrap();

    assert_eq!(article.title, "Test Article");
    assert_eq!(article.author, Some("Test Author".to_string()));
    assert_eq!(article.word_count, 150);
    assert_eq!(article.reading_time_minutes, 1);
}

#[test]
fn test_generate_fallback_title_with_bad_title() {
    let result = WebArchiveService::generate_fallback_title(
        "https://reddit.com/r/rust/comments/abc123/some_discussion",
        "The heart of the internet",
    );
    assert!(result.contains("reddit.com"));
    assert_ne!(result, "The heart of the internet");

    let result = WebArchiveService::generate_fallback_title("https://example.com/article", "Short");
    assert!(result.contains("example.com"));

    let result = WebArchiveService::generate_fallback_title(
        "https://github.com/rust-lang/rust/pull/12345",
        "Loading",
    );
    assert!(result.contains("12345"));
    assert!(result.contains("github.com"));
}

#[test]
fn test_generate_fallback_title_with_good_title() {
    // Good title should pass through unchanged
    let good_title = "A Comprehensive Guide to Rust Programming";
    let result = WebArchiveService::generate_fallback_title("https://example.com", good_title);
    assert_eq!(result, good_title);
}
