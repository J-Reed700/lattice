use super::*;
use lattice_core::application::contracts::web::CleanArticle;

#[test]
fn owns_refuses_a_backslash_separated_parent_on_every_platform() {
    let temp = tempfile::tempdir().expect("temp dir");
    let base = temp.path().join("web-archive");
    std::fs::create_dir_all(&base).expect("archive directory");
    let service = WebArchiveService::with_base_dir(base.clone());
    assert!(!service.owns(&base.join("a\\..\\..\\x")));
}

#[cfg(windows)]
#[test]
fn owns_refuses_a_dots_only_segment_on_windows() {
    let temp = tempfile::tempdir().expect("temp dir");
    let base = temp.path().join("web-archive");
    std::fs::create_dir_all(&base).expect("archive directory");
    let service = WebArchiveService::with_base_dir(base.clone());
    assert!(!service.owns(&base.join("...").join("x")));
}

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

fn lay_out_article(dir: &Path, metadata: Option<&str>, page: bool) -> PathBuf {
    std::fs::create_dir_all(dir).expect("article directory");
    let text = dir.join("article.md");
    std::fs::write(&text, "body").expect("write text");
    if page {
        std::fs::write(dir.join("page.html"), "<html></html>").expect("write page");
    }
    if let Some(metadata) = metadata {
        std::fs::write(dir.join("metadata.json"), metadata).expect("write metadata");
    }
    text
}

const FULL_METADATA: &str = r#"{
    "url": "https://example.com/essay",
    "title": "The Essay",
    "links": ["https://example.com/a", "https://example.com/b"]
}"#;

#[tokio::test]
async fn an_article_laid_out_in_the_archive_gives_its_text_page_title_url_and_links() {
    let temp = tempfile::tempdir().expect("temp dir");
    let service = WebArchiveService::with_base_dir(temp.path().to_path_buf());
    let dir = temp.path().join("example.com").join("essay-1");
    let text = lay_out_article(&dir, Some(FULL_METADATA), true);

    let article = service.saved_article(&text).await.expect("a saved article");

    assert_eq!(article.text, text);
    assert_eq!(article.page, Some(dir.join("page.html")));
    assert_eq!(article.title.as_deref(), Some("The Essay"));
    assert_eq!(article.url.as_deref(), Some("https://example.com/essay"));
    assert_eq!(
        article.links,
        vec!["https://example.com/a", "https://example.com/b"]
    );
}

#[tokio::test]
async fn the_same_three_files_outside_the_archive_give_no_article() {
    let temp = tempfile::tempdir().expect("temp dir");
    let base = temp.path().join("web-archive");
    std::fs::create_dir_all(&base).expect("archive directory");
    let service = WebArchiveService::with_base_dir(base);
    let text = lay_out_article(&temp.path().join("elsewhere"), Some(FULL_METADATA), true);

    assert_eq!(service.saved_article(&text).await, None);
}

#[tokio::test]
async fn a_page_that_is_gone_gives_no_page_and_keeps_the_text() {
    let temp = tempfile::tempdir().expect("temp dir");
    let service = WebArchiveService::with_base_dir(temp.path().to_path_buf());
    let dir = temp.path().join("example.com").join("essay-1");
    let text = lay_out_article(&dir, Some(FULL_METADATA), false);

    let article = service.saved_article(&text).await.expect("a saved article");

    assert_eq!(article.text, text);
    assert_eq!(article.page, None);
}

#[tokio::test]
async fn a_metadata_file_holding_only_a_title_still_gives_the_title() {
    let temp = tempfile::tempdir().expect("temp dir");
    let service = WebArchiveService::with_base_dir(temp.path().to_path_buf());
    let dir = temp.path().join("example.com").join("essay-1");
    let text = lay_out_article(&dir, Some(r#"{"title": "Only A Title"}"#), true);

    let article = service.saved_article(&text).await.expect("a saved article");

    assert_eq!(article.title.as_deref(), Some("Only A Title"));
    assert_eq!(article.url, None);
    assert!(article.links.is_empty());
}

#[tokio::test]
async fn an_unparsable_metadata_file_still_gives_the_article_without_a_title() {
    let temp = tempfile::tempdir().expect("temp dir");
    let service = WebArchiveService::with_base_dir(temp.path().to_path_buf());
    let dir = temp.path().join("example.com").join("essay-1");
    let text = lay_out_article(&dir, Some("invalid json"), true);

    let article = service.saved_article(&text).await.expect("a saved article");

    assert_eq!(article.text, text);
    assert_eq!(article.page, Some(dir.join("page.html")));
    assert_eq!(article.title, None);
    assert_eq!(article.url, None);
    assert!(article.links.is_empty());
}

#[tokio::test]
async fn a_path_that_climbs_out_of_the_archive_with_parent_segments_gives_no_article() {
    let temp = tempfile::tempdir().expect("temp dir");
    let base = temp.path().join("web-archive");
    std::fs::create_dir_all(&base).expect("archive directory");
    let service = WebArchiveService::with_base_dir(base.clone());
    lay_out_article(&temp.path().join("outside"), Some(FULL_METADATA), true);
    let climbing = base
        .join("example.com")
        .join("..")
        .join("..")
        .join("outside")
        .join("article.md");

    assert_eq!(service.saved_article(&climbing).await, None);
}

#[tokio::test]
async fn the_page_file_of_an_archived_article_gives_no_article() {
    let temp = tempfile::tempdir().expect("temp dir");
    let service = WebArchiveService::with_base_dir(temp.path().to_path_buf());
    let dir = temp.path().join("example.com").join("essay-1");
    lay_out_article(&dir, Some(FULL_METADATA), true);

    assert_eq!(service.saved_article(&dir.join("page.html")).await, None);
}

#[tokio::test]
async fn a_base_directory_that_was_never_canonicalized_still_holds_the_canonical_spelling_of_its_article(
) {
    let temp = tempfile::tempdir().expect("temp dir");
    let service = WebArchiveService::with_base_dir(temp.path().to_path_buf());
    let dir = temp.path().join("example.com").join("essay-1");
    let text = lay_out_article(&dir, Some(FULL_METADATA), true);
    let canonical = text.canonicalize().expect("canonical article");
    #[cfg(windows)]
    assert!(
        canonical.to_string_lossy().starts_with("\\\\?\\"),
        "the canonical spelling carries the verbatim prefix: {}",
        canonical.display()
    );

    let article = service
        .saved_article(&canonical)
        .await
        .expect("a saved article");

    assert_eq!(article.title.as_deref(), Some("The Essay"));
}

#[tokio::test]
async fn a_canonical_base_directory_still_holds_the_raw_spelling_of_its_article() {
    let temp = tempfile::tempdir().expect("temp dir");
    let base = temp.path().canonicalize().expect("canonical base");
    let service = WebArchiveService::with_base_dir(base);
    let dir = temp.path().join("example.com").join("essay-1");
    let text = lay_out_article(&dir, Some(FULL_METADATA), true);
    #[cfg(windows)]
    assert_ne!(
        text,
        text.canonicalize().expect("canonical article"),
        "the raw spelling differs from the canonical one"
    );

    let article = service.saved_article(&text).await.expect("a saved article");

    assert_eq!(article.title.as_deref(), Some("The Essay"));
}

#[test]
fn owns_a_path_inside_the_archive_however_it_is_spelled() {
    let temp = tempfile::tempdir().expect("temp dir");
    let base = temp.path().to_path_buf();
    let text = lay_out_article(&base.join("a1"), None, false);
    let canonical = std::fs::canonicalize(&text).expect("canonical article");
    #[cfg(windows)]
    assert_ne!(
        text, canonical,
        "the raw spelling differs from the canonical one"
    );

    let raw_base = WebArchiveService::with_base_dir(base.clone());
    assert!(raw_base.owns(&text));
    assert!(raw_base.owns(&canonical));

    let canonical_base =
        WebArchiveService::with_base_dir(std::fs::canonicalize(&base).expect("canonical base"));
    assert!(canonical_base.owns(&text));
}

#[test]
fn still_owns_an_article_whose_file_is_already_gone() {
    let temp = tempfile::tempdir().expect("temp dir");
    let base = temp.path().to_path_buf();
    std::fs::create_dir_all(base.join("a1")).expect("article directory");
    let service = WebArchiveService::with_base_dir(base.clone());

    assert!(service.owns(&base.join("a1").join("article.md")));
}

#[test]
fn owns_a_path_under_an_archive_folder_that_does_not_exist_yet() {
    let temp = tempfile::tempdir().expect("temp dir");
    let base = temp.path().join("missing").join("web-archive");
    let service = WebArchiveService::with_base_dir(base.clone());

    assert!(service.owns(&base.join("some-article").join("article.md")));
}

#[test]
fn only_an_article_text_inside_the_archive_is_a_saved_article() {
    let temp = tempfile::tempdir().expect("temp dir");
    let outside = tempfile::tempdir().expect("temp dir");
    let base = temp.path().join("web-archive");
    let service = WebArchiveService::with_base_dir(base.clone());

    assert!(service.is_saved_article(&base.join("some-article").join("article.md")));
    assert!(!service.is_saved_article(&base.join("example.com").join("old-article-1.md")));
    assert!(!service.is_saved_article(&base.join("some-article").join("page.html")));
    assert!(!service.is_saved_article(&outside.path().join("some-article").join("article.md")));
}
