use super::*;

#[test]
fn test_sanitize_title_basic() {
    assert_eq!(sanitize_title("Hello World"), "hello-world");
}

#[test]
fn test_sanitize_title_with_special_chars() {
    assert_eq!(sanitize_title("Hello, World! 2024"), "hello-world-2024");
}

#[test]
fn test_sanitize_title_removes_consecutive_hyphens() {
    assert_eq!(sanitize_title("Hello---World"), "hello-world");
}

#[test]
fn test_sanitize_title_truncates_long_titles() {
    let long_title = "a".repeat(100);
    let sanitized = sanitize_title(&long_title);
    assert_eq!(sanitized.len(), 50);
}

#[test]
fn test_sanitize_title_removes_leading_trailing_hyphens() {
    assert_eq!(sanitize_title("--Hello World--"), "hello-world");
}

#[test]
fn test_web_archive_path_creation() {
    let url = Url::parse("https://github.com/rust-lang/rust").unwrap();
    let doc_id = DocumentId::new();
    let path = WebArchivePath::new(&url, "Rust Guide", &doc_id).unwrap();

    // The domain is the archive path's first *component*, not a literal
    // `github.com/` prefix: `relative_path` is a `PathBuf`, so on Windows it
    // renders as `github.com\…` and a string prefix check fails even though
    // the structure is correct. Assert on components and on the file name.
    let relative = path.as_path();
    assert!(
        relative.starts_with("github.com"),
        "{} should be under the domain directory",
        relative.display()
    );
    let file_name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .expect("archive path must have a file name");
    assert!(file_name.contains("rust-guide"));
    assert_eq!(
        relative.extension().and_then(|ext| ext.to_str()),
        Some("md")
    );
}

#[test]
fn test_web_archive_path_rejects_url_without_host() {
    let url = Url::parse("file:///local/path").unwrap();
    let doc_id = DocumentId::new();
    let result = WebArchivePath::new(&url, "Test", &doc_id);

    assert!(result.is_err());
}

#[test]
fn test_web_archive_path_rejects_empty_title() {
    let url = Url::parse("https://example.com").unwrap();
    let doc_id = DocumentId::new();
    let result = WebArchivePath::new(&url, "", &doc_id);

    assert!(result.is_err());
}

#[test]
fn test_web_archive_path_to_absolute() {
    use std::env;
    let temp_dir = env::temp_dir().join("test_web_archive_to_absolute");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let url = Url::parse("https://example.com/article").unwrap();
    let doc_id = DocumentId::new();
    let path = WebArchivePath::new(&url, "Example", &doc_id).unwrap();

    let absolute = path.to_absolute(&temp_dir).unwrap();

    // Canonicalize temp_dir for comparison (to_absolute canonicalizes internally)
    let canonical_temp = temp_dir.canonicalize().unwrap();
    assert!(absolute.starts_with(&canonical_temp));
    assert!(absolute.to_str().unwrap().contains("example.com"));

    std::fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_article_metadata_from_content() {
    let url = Url::parse("https://example.com").unwrap();
    let content = "This is a test article with five words";
    let metadata = ArticleMetadata::from_content(content, &url, None);

    assert_eq!(metadata.word_count, 8);
    assert_eq!(metadata.domain, "example.com");
    assert!(metadata.published_date.is_none());
}

#[test]
fn test_web_article_from_ingestion() {
    let url = Url::parse("https://blog.rust-lang.org/article").unwrap();
    let content = IngestedContent {
        html: "<html>test</html>".to_string(),
        text: "Article content here".to_string(),
        title: Some("Rust Article".to_string()),
        author: Some("Rust Team".to_string()),
        published_date: None,
    };

    let article = WebArticleAggregate::from_ingestion(url, content).unwrap();

    assert_eq!(article.title(), "Rust Article");
    assert_eq!(article.author(), Some("Rust Team"));
    assert_eq!(article.content(), "Article content here");
}

#[test]
fn test_web_article_rejects_empty_content() {
    let url = Url::parse("https://example.com").unwrap();
    let content = IngestedContent {
        html: "<html></html>".to_string(),
        text: "".to_string(),
        title: Some("Test".to_string()),
        author: None,
        published_date: None,
    };

    let result = WebArticleAggregate::from_ingestion(url, content);
    assert!(result.is_err());
}

#[test]
fn test_web_article_rejects_missing_title() {
    let url = Url::parse("https://example.com").unwrap();
    let content = IngestedContent {
        html: "<html></html>".to_string(),
        text: "Content".to_string(),
        title: None,
        author: None,
        published_date: None,
    };

    let result = WebArticleAggregate::from_ingestion(url, content);
    assert!(result.is_err());
}

#[test]
fn test_web_article_archive_path() {
    let url = Url::parse("https://example.com/article").unwrap();
    let content = IngestedContent {
        html: "<html></html>".to_string(),
        text: "Content here".to_string(),
        title: Some("Example Article".to_string()),
        author: None,
        published_date: None,
    };

    let article = WebArticleAggregate::from_ingestion(url, content).unwrap();
    let path = article.archive_path().unwrap();

    assert!(path.as_str().contains("example.com"));
    assert!(path.as_str().contains("example-article"));
}

#[test]
fn test_web_article_to_markdown() {
    let url = Url::parse("https://example.com/article").unwrap();
    let content = IngestedContent {
        html: "<html></html>".to_string(),
        text: "Article content goes here".to_string(),
        title: Some("Test Article".to_string()),
        author: Some("John Doe".to_string()),
        published_date: None,
    };

    let article = WebArticleAggregate::from_ingestion(url, content).unwrap();
    let markdown = article.to_markdown();

    assert!(markdown.contains("---"));
    assert!(markdown.contains("source_url: https://example.com/article"));
    assert!(markdown.contains("title: Test Article"));
    assert!(markdown.contains("author: John Doe"));
    assert!(markdown.contains("domain: example.com"));
    assert!(markdown.contains("word_count: 4"));

    assert!(markdown.contains("Article content goes here"));

    assert!(markdown.contains("Source: https://example.com/article"));
}

#[test]
fn test_web_article_markdown_without_author() {
    let url = Url::parse("https://example.com").unwrap();
    let content = IngestedContent {
        html: "<html></html>".to_string(),
        text: "Content".to_string(),
        title: Some("Title".to_string()),
        author: None,
        published_date: None,
    };

    let article = WebArticleAggregate::from_ingestion(url, content).unwrap();
    let markdown = article.to_markdown();

    assert!(!markdown.contains("author:"));
}

#[test]
fn test_full_web_article_workflow() {
    let url = Url::parse("https://blog.example.com/2024/rust-tips").unwrap();
    let content = IngestedContent {
        html: "<html><body>Rust tips</body></html>".to_string(),
        text: "Here are some useful Rust programming tips".to_string(),
        title: Some("Rust Programming Tips".to_string()),
        author: Some("Jane Developer".to_string()),
        published_date: None,
    };

    let article = WebArticleAggregate::from_ingestion(url, content).unwrap();

    let path = article.archive_path().unwrap();
    assert!(path.as_str().contains("blog.example.com"));
    assert!(path.as_str().contains("rust-programming-tips"));

    let markdown = article.to_markdown();
    assert!(markdown.contains("title: Rust Programming Tips"));
    assert!(markdown.contains("author: Jane Developer"));
    assert!(markdown.contains("Here are some useful Rust programming tips"));

    assert_eq!(article.metadata().domain, "blog.example.com");
    assert!(article.metadata().word_count > 0);
}

#[test]
fn test_validate_domain_rejects_traversal() {
    assert!(validate_domain("..").is_err());
    assert!(validate_domain(".").is_err());
    assert!(validate_domain("evil/../etc").is_err());
    assert!(validate_domain("evil\\..\\etc").is_err()); // Windows-style
    assert!(validate_domain("...").is_err()); // Just dots
}

#[test]
fn test_validate_domain_rejects_special_chars() {
    assert!(validate_domain("example.com/path").is_err());
    assert!(validate_domain("evil\\backslash").is_err());
    assert!(validate_domain("null\0char").is_err());
    assert!(validate_domain("bad:colon").is_err());
    assert!(validate_domain("quote\"here").is_err());
    assert!(validate_domain("pipe|here").is_err());
    assert!(validate_domain("question?mark").is_err());
    assert!(validate_domain("asterisk*here").is_err());
    assert!(validate_domain("<angle>").is_err());
}

#[test]
fn test_validate_domain_rejects_control_chars() {
    assert!(validate_domain("evil\nline").is_err());
    assert!(validate_domain("evil\rcarriage").is_err());
    assert!(validate_domain("evil\ttab").is_err());
    assert!(validate_domain("evil\x00null").is_err());
}

#[test]
fn test_validate_domain_trims_whitespace() {
    assert!(validate_domain("  ").is_err()); // Only whitespace
    assert!(validate_domain("   example.com   ").is_ok()); // Should trim and accept
}

#[test]
fn test_validate_domain_accepts_valid() {
    assert!(validate_domain("example.com").is_ok());
    assert!(validate_domain("sub.example.com").is_ok());
    assert!(validate_domain("localhost").is_ok());
    assert!(validate_domain("192.168.1.1").is_ok());
    assert!(validate_domain("api-v2.example.com").is_ok()); // Hyphens ok
}

#[test]
fn test_validate_domain_rejects_empty() {
    assert!(validate_domain("").is_err());
}

#[test]
fn test_validate_domain_rejects_too_long() {
    let long_domain = "a".repeat(256);
    assert!(validate_domain(&long_domain).is_err());
}

#[test]
fn test_web_archive_path_rejects_malicious_domain() {
    let doc_id = DocumentId::new();

    let file_url = Url::parse("file:///local/path").unwrap();
    let result = WebArchivePath::new(&file_url, "Test", &doc_id);
    assert!(result.is_err());
}

#[test]
fn test_web_archive_path_rejects_domain_with_slash() {
    // This would be caught by URL parsing, but if we somehow get a domain with slash
    // our validation will catch it
    // Since url::Url won't parse invalid URLs, we can't directly test this case
    // but the validate_domain function is tested independently above
}

#[test]
fn test_to_absolute_prevents_path_traversal() {
    use std::env;
    let temp_dir = env::temp_dir().join("test_web_archive_toctou");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let malicious1 = WebArchivePath {
        relative_path: PathBuf::from("../../../etc/passwd"),
    };
    assert!(
        malicious1.to_absolute(&temp_dir).is_err(),
        "Should reject path with .."
    );

    let malicious2 = WebArchivePath {
        relative_path: PathBuf::from("safe/../../etc/passwd"),
    };
    assert!(
        malicious2.to_absolute(&temp_dir).is_err(),
        "Should reject mixed path with .."
    );

    let valid = WebArchivePath {
        relative_path: PathBuf::from("github.com/article.md"),
    };
    let valid_result = valid.to_absolute(&temp_dir);
    assert!(valid_result.is_ok(), "Should accept valid path");

    let valid_path = valid_result.unwrap();
    assert!(
        valid_path.parent().unwrap().exists(),
        "Parent directory should be created"
    );

    let canonical_temp = temp_dir.canonicalize().unwrap();
    assert!(
        valid_path.starts_with(&canonical_temp),
        "Valid path should be within base directory"
    );

    std::fs::remove_dir_all(&temp_dir).ok();
}
