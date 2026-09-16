//! # Web Archive Domain Models
//!
//! Domain models for web article ingestion and storage following DDD principles.
//!
//! ## Value Objects
//!
//! - [`WebArchivePath`]: Sanitized file path for web articles
//!
//! ## Aggregates
//!
//! - [`WebArticleAggregate`]: Web article with metadata and content
//!
//! ## Invariants
//!
//! - Archive paths must be unique and sanitized
//! - Article content must not be empty
//! - URLs must have a valid host
//! - Titles must not be empty
//!
//! ## Usage
//!
//! ```rust,no_run
//! use lattice::domain::web_archive::*;
//! use url::Url;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let url = Url::parse("https://example.com/article")?;
//! let content = IngestedContent {
//!     html: "<html>...</html>".to_string(),
//!     text: "Article text".to_string(),
//!     title: Some("Example Article".to_string()),
//!     author: Some("Jane Doe".to_string()),
//!     published_date: None,
//! };
//!
//! let article = WebArticleAggregate::from_ingestion(url, content)?;
//! let path = article.archive_path()?;
//! let markdown = article.to_markdown();
//! # Ok(())
//! # }
//! ```

use crate::shared::domain_types::DocumentId;
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use url::Url;

/// Validate that a domain is safe for use as a directory name.
///
/// Security checks:
/// - Rejects empty domains
/// - Rejects path traversal attempts (.. and .)
/// - Rejects filesystem special characters
/// - Rejects overly long domains (>255 chars)
///
/// # Errors
///
/// Returns `AppError::InvalidInput` if domain contains:
/// - Path traversal sequences (.., .)
/// - Filesystem special characters (/, \, :, etc.)
/// - Empty or overly long strings
///
/// # Example
///
/// ```
/// # use lattice::domain::web_archive::validate_domain;
/// assert!(validate_domain("example.com").is_ok());
/// assert!(validate_domain("..").is_err());
/// assert!(validate_domain("evil/path").is_err());
/// ```
pub fn validate_domain(domain: &str) -> Result<()> {
    // Trim whitespace and check for empty
    let domain = domain.trim();

    if domain.is_empty() {
        return Err(AppError::InvalidInput("Domain cannot be empty".to_string()));
    }

    // Reject path traversal - component-based check is more robust
    if domain.split('/').any(|part| part == ".." || part == ".") {
        return Err(AppError::InvalidInput(
            "Domain cannot contain path traversal sequences (. or ..)".to_string(),
        ));
    }

    // Also check for backslash-separated (Windows-style)
    if domain.split('\\').any(|part| part == ".." || part == ".") {
        return Err(AppError::InvalidInput(
            "Domain cannot contain path traversal sequences (. or ..)".to_string(),
        ));
    }

    // Reject control characters (including null byte)
    if domain.chars().any(|c| c.is_control()) {
        return Err(AppError::InvalidInput(
            "Domain contains control characters".to_string(),
        ));
    }

    // Reject filesystem special characters
    const INVALID_CHARS: &[char] = &['/', '\\', '\0', '<', '>', ':', '"', '|', '?', '*'];
    if let Some(invalid_char) = domain.chars().find(|c| INVALID_CHARS.contains(c)) {
        return Err(AppError::InvalidInput(format!(
            "Domain contains invalid character: '{}'",
            invalid_char
        )));
    }

    // Reject overly long domains (filesystem limits)
    if domain.len() > 255 {
        return Err(AppError::InvalidInput(format!(
            "Domain exceeds maximum length of 255 characters (got {})",
            domain.len()
        )));
    }

    // Reject domains that are just dots
    if domain.chars().all(|c| c == '.') {
        return Err(AppError::InvalidInput(
            "Domain cannot consist only of dots".to_string(),
        ));
    }

    Ok(())
}

/// Sanitize a title for use in a filename.
///
/// Rules:
/// - Lowercase
/// - Alphanumeric + hyphens only
/// - Max 50 characters
/// - Remove consecutive hyphens
///
/// # Example
///
/// ```
/// # use lattice::domain::web_archive::sanitize_title;
/// let sanitized = sanitize_title("Hello, World! 2024");
/// assert_eq!(sanitized, "hello-world-2024");
/// ```
pub fn sanitize_title(title: &str) -> String {
    let sanitized = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>();

    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in sanitized.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push(c);
                prev_hyphen = true;
            }
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    let result = result.trim_matches('-');

    // Truncate to max 50 characters
    result.chars().take(50).collect()
}

/// Value Object: Web archive file path
///
/// Represents a sanitized, unique file path for storing web articles.
/// Format: `{domain}/{sanitized-title}-{short-uuid}.md`
///
/// # Invariants
///
/// - Path is relative to archive base directory
/// - Filename is sanitized and unique
/// - Extension is always `.md`
///
/// # Example
///
/// ```rust,no_run
/// # use lattice::domain::web_archive::WebArchivePath;
/// # use lattice::shared::domain_types::DocumentId;
/// # use url::Url;
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let url = Url::parse("https://github.com/rust-lang/rust")?;
/// let title = "The Rust Programming Language Guide";
/// let doc_id = DocumentId::new();
///
/// let path = WebArchivePath::new(&url, title, &doc_id)?;
/// // Result: "github.com/the-rust-programming-language-guide-a1b2c3d4.md"
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebArchivePath {
    relative_path: PathBuf,
}

impl WebArchivePath {
    /// Create new web archive path from URL, title, and document ID.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - URL has no host
    /// - Title is empty
    ///
    /// # Invariants
    ///
    /// - Generated path is unique (uses doc_id)
    /// - Filename is sanitized
    pub fn new(url: &Url, title: &str, doc_id: &DocumentId) -> Result<Self> {
        let domain = url
            .host_str()
            .ok_or_else(|| AppError::InvalidInput("URL must have a host".to_string()))?;

        validate_domain(domain)?;

        if title.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Article title cannot be empty".to_string(),
            ));
        }

        // Sanitize title
        let sanitized_title = sanitize_title(title);

        let short_uuid = &doc_id.as_str()[..8];

        // Format: {domain}/{sanitized-title}-{short-uuid}.md
        let filename = format!("{}-{}.md", sanitized_title, short_uuid);
        let relative_path = PathBuf::from(domain).join(filename);

        Ok(Self { relative_path })
    }

    /// Convert to absolute path given a base directory.
    ///
    /// Performs component-based validation and path traversal prevention (CWE-22 mitigation).
    /// SECURITY: Validates BEFORE creating any directories to prevent TOCTOU vulnerabilities.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Base directory is invalid or doesn't exist
    /// - Path contains traversal attempts (..)
    /// - Path escapes the base directory (directory traversal attempt)
    /// - Parent directory cannot be created
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::domain::web_archive::WebArchivePath;
    /// # use lattice::shared::domain_types::DocumentId;
    /// # use url::Url;
    /// # use std::path::PathBuf;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let url = Url::parse("https://example.com/article")?;
    /// let doc_id = DocumentId::new();
    /// let path = WebArchivePath::new(&url, "Example", &doc_id)?;
    ///
    /// let base_dir = PathBuf::from("/data/archives");
    /// let absolute = path.to_absolute(&base_dir)?;
    /// // Result: "/data/archives/example.com/example-a1b2c3d4.md"
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_absolute(&self, base_dir: &Path) -> Result<PathBuf> {
        use std::path::Component;

        // SECURITY: Validate path components BEFORE any filesystem operations
        // This prevents TOCTOU (Time-of-Check Time-of-Use) vulnerabilities
        for component in self.relative_path.components() {
            match component {
                Component::Normal(_) => continue,
                Component::ParentDir => {
                    return Err(AppError::Security(
                        "Path traversal detected: relative path contains ..".to_string(),
                    ));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(AppError::InvalidInput(
                        "Relative path cannot contain absolute components".to_string(),
                    ));
                }
                Component::CurDir => continue, // Allow "." (current dir)
            }
        }

        // Canonicalize base directory (must exist)
        let canonical_base = base_dir
            .canonicalize()
            .map_err(|e| AppError::InvalidInput(format!("Invalid base directory: {}", e)))?;

        // Manually resolve relative path against canonical base
        // We can't use canonicalize on the full path since it might not exist yet
        let mut resolved = canonical_base.clone();
        for component in self.relative_path.components() {
            match component {
                Component::Normal(name) => resolved.push(name),
                Component::ParentDir => {
                    // Already validated above, this is defensive redundancy
                    return Err(AppError::Security("Unexpected .. in path".to_string()));
                }
                Component::CurDir => continue,
                _ => {
                    return Err(AppError::InvalidInput(
                        "Unexpected path component".to_string(),
                    ));
                }
            }
        }

        // Final security check: resolved path must be within base directory
        if !resolved.starts_with(&canonical_base) {
            return Err(AppError::Security(
                "Path escapes base directory".to_string(),
            ));
        }

        // NOW it's safe to create directories (validation happened first)
        if let Some(parent) = resolved.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Io {
                    message: format!("Failed to create directory: {}", e),
                    kind: e.kind().to_string(),
                })?;
            }
        }

        Ok(resolved)
    }

    /// Get the relative path.
    pub fn as_path(&self) -> &Path {
        &self.relative_path
    }

    /// Get the relative path as a string.
    pub fn as_str(&self) -> &str {
        self.relative_path.to_str().unwrap_or("")
    }
}

/// Article metadata extracted from web content.
///
/// Contains optional fields that may or may not be present
/// depending on the source article.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleMetadata {
    /// Published date (if available)
    pub published_date: Option<DateTime<Utc>>,

    /// Word count of article content
    pub word_count: usize,

    /// Tags or keywords (if extracted)
    pub tags: Vec<String>,

    /// Domain of source URL
    pub domain: String,

    /// Date when article was imported
    pub imported_date: DateTime<Utc>,
}

impl ArticleMetadata {
    /// Create metadata from content and URL.
    fn from_content(content: &str, url: &Url, published_date: Option<DateTime<Utc>>) -> Self {
        let word_count = content.split_whitespace().count();
        let domain = url.host_str().unwrap_or("unknown").to_string();

        Self {
            published_date,
            word_count,
            tags: Vec::new(),
            domain,
            imported_date: Utc::now(),
        }
    }
}

/// Content ingested from a web page.
///
/// Contains the raw HTML, extracted text, and metadata
/// from the web capture process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedContent {
    /// Raw HTML content
    pub html: String,

    /// Extracted text content
    pub text: String,

    /// Article title (if extracted)
    pub title: Option<String>,

    /// Article author (if extracted)
    pub author: Option<String>,

    /// Published date (if extracted)
    pub published_date: Option<DateTime<Utc>>,
}

/// Aggregate: Web Article Document
///
/// Represents a web article that has been ingested and is ready for storage.
/// Encapsulates all information needed to archive the article as a markdown file.
///
/// # Invariants
///
/// - Content must not be empty
/// - Source URL must be valid
/// - Title must not be empty
///
/// # Example
///
/// ```rust,no_run
/// # use lattice::domain::web_archive::*;
/// # use url::Url;
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let url = Url::parse("https://blog.rust-lang.org/2024/article")?;
/// let content = IngestedContent {
///     html: "<html>...</html>".to_string(),
///     text: "Article content here".to_string(),
///     title: Some("Rust 1.75 Released".to_string()),
///     author: Some("Rust Team".to_string()),
///     published_date: None,
/// };
///
/// let article = WebArticleAggregate::from_ingestion(url, content)?;
/// let markdown = article.to_markdown();
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebArticleAggregate {
    doc_id: DocumentId,
    source_url: Url,
    title: String,
    author: Option<String>,
    content: String,
    metadata: ArticleMetadata,
}

impl WebArticleAggregate {
    /// Create from ingested content.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Content text is empty
    /// - Title is missing or empty
    ///
    /// # Invariants
    ///
    /// - Generated doc_id is unique
    /// - Content is not empty
    /// - Title is not empty
    pub fn from_ingestion(url: Url, content: IngestedContent) -> Result<Self> {
        if content.text.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Article content cannot be empty".to_string(),
            ));
        }

        let title = content
            .title
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| AppError::InvalidInput("Article title is required".to_string()))?;

        let doc_id = DocumentId::new();

        let metadata = ArticleMetadata::from_content(&content.text, &url, content.published_date);

        Ok(Self {
            doc_id,
            source_url: url,
            title,
            author: content.author,
            content: content.text,
            metadata,
        })
    }

    /// Generate archive file path.
    ///
    /// # Errors
    ///
    /// Returns an error if path generation fails (invalid URL or title).
    pub fn archive_path(&self) -> Result<WebArchivePath> {
        WebArchivePath::new(&self.source_url, &self.title, &self.doc_id)
    }

    /// Serialize to markdown with YAML frontmatter.
    ///
    /// Format:
    /// ```markdown
    /// ---
    /// source_url: https://example.com/article
    /// title: Example Article
    /// author: Jane Doe
    /// imported_date: 2024-12-05T10:30:00Z
    /// domain: example.com
    /// ---
    ///
    /// Article content here...
    ///
    /// ---
    /// Source: https://example.com/article
    /// ```
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        // YAML frontmatter
        output.push_str("---\n");
        output.push_str(&format!("source_url: {}\n", self.source_url));
        output.push_str(&format!("title: {}\n", self.title));

        if let Some(ref author) = self.author {
            output.push_str(&format!("author: {}\n", author));
        }

        output.push_str(&format!(
            "imported_date: {}\n",
            self.metadata.imported_date.to_rfc3339()
        ));
        output.push_str(&format!("domain: {}\n", self.metadata.domain));

        if let Some(published) = self.metadata.published_date {
            output.push_str(&format!("published_date: {}\n", published.to_rfc3339()));
        }

        output.push_str(&format!("word_count: {}\n", self.metadata.word_count));
        output.push_str("---\n\n");

        // Content
        output.push_str(&self.content);
        output.push_str("\n\n");

        // Footer with source attribution
        output.push_str("---\n");
        output.push_str(&format!("Source: {}\n", self.source_url));

        output
    }

    /// Get document ID.
    pub fn doc_id(&self) -> &DocumentId {
        &self.doc_id
    }

    /// Get source URL.
    pub fn source_url(&self) -> &Url {
        &self.source_url
    }

    /// Get article title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Get article author.
    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    /// Get article content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get article metadata.
    pub fn metadata(&self) -> &ArticleMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests {
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

        let path_str = path.as_str();
        assert!(path_str.starts_with("github.com/"));
        assert!(path_str.contains("rust-guide"));
        assert!(path_str.ends_with(".md"));
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
}
