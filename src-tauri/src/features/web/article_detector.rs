//! Web Article Detector
//!
//! Detects web articles archived by the browser extension and extracts metadata.
//!
//! ## Web Article Structure
//!
//! A web article is identified by:
//! - Main file: `article.md` or `weblink.md` (Markdown extraction)
//! - Sibling file: `page.html` (HTML snapshot)
//! - Metadata file: `metadata.json` (article metadata)
//!
//! Example directory structure:
//! ```text
//! web-archives/
//! └── example-article/
//!     ├── article.md       # Main content (Markdown)
//!     ├── page.html        # HTML snapshot
//!     └── metadata.json    # Title, URL, site_name
//! ```

use crate::shared::error::AppError;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Article metadata extracted from `metadata.json`
#[derive(Debug, Deserialize)]
struct ArticleMetadata {
    /// Article title
    title: String,

    /// Original URL
    // reason: required key of the browser extension's metadata.json; keeping it
    // preserves the parse-time check that the file actually carries a URL.
    #[allow(dead_code)]
    url: String,

    /// Optional site name (e.g., "Wikipedia", "Medium")
    // reason: part of the same metadata.json schema, kept alongside `url`.
    #[allow(dead_code)]
    #[serde(default)]
    site_name: Option<String>,
}

/// Detector for web articles archived by browser extension
pub struct WebArticleDetector;

impl WebArticleDetector {
    fn is_supported_markdown_name(path: &Path) -> bool {
        matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("article.md") | Some("weblink.md")
        )
    }

    /// Check if a path is a web archive article
    ///
    /// Returns `true` if:
    /// 1. Filename is exactly `article.md` or `weblink.md`
    /// 2. Sibling file `page.html` exists
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::path::Path;
    /// use lattice::features::web::article_detector::WebArticleDetector;
    ///
    /// let article_path = Path::new("web-archives/my-article/article.md");
    /// if WebArticleDetector::is_web_article(article_path) {
    ///     println!("This is a web article!");
    /// }
    /// ```
    pub fn is_web_article(path: &Path) -> bool {
        // Must be named "article.md" or "weblink.md"
        if !Self::is_supported_markdown_name(path) {
            return false;
        }

        // Must have sibling "page.html"
        if let Some(dir) = path.parent() {
            dir.join("page.html").exists()
        } else {
            false
        }
    }

    /// Get path to HTML snapshot for web article
    ///
    /// Returns `Some(PathBuf)` if `page.html` exists in same directory,
    /// otherwise `None`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::path::Path;
    /// use lattice::features::web::article_detector::WebArticleDetector;
    ///
    /// let article_path = Path::new("web-archives/my-article/article.md");
    /// if let Some(html_path) = WebArticleDetector::get_html_path(article_path) {
    ///     println!("HTML snapshot: {}", html_path.display());
    /// }
    /// ```
    pub fn get_html_path(article_md_path: &Path) -> Option<PathBuf> {
        let dir = article_md_path.parent()?;
        let html_path = dir.join("page.html");
        if html_path.exists() {
            Some(html_path)
        } else {
            None
        }
    }

    /// Extract title from `metadata.json`
    ///
    /// Reads and parses `metadata.json` in the same directory as the article,
    /// extracting the title field.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` - Invalid article path (no parent directory)
    /// - `AppError::NotFound` - `metadata.json` not found
    /// - `AppError::Io` - Failed to read file
    /// - `AppError::Serialization` - Failed to parse JSON
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::path::Path;
    /// use lattice::features::web::article_detector::WebArticleDetector;
    ///
    /// # async fn example() -> Result<(), lattice::shared::error::error::AppError> {
    /// let article_path = Path::new("web-archives/my-article/article.md");
    /// let title = WebArticleDetector::get_title(article_path).await?;
    /// println!("Article title: {}", title);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_title(article_md_path: &Path) -> Result<String, AppError> {
        let dir = article_md_path
            .parent()
            .ok_or_else(|| AppError::InvalidInput("Invalid article path".to_string()))?;

        let metadata_path = dir.join("metadata.json");
        if !metadata_path.exists() {
            return Err(AppError::NotFound("metadata.json not found".to_string()));
        }

        let content = fs::read_to_string(&metadata_path)
            .await
            .map_err(|e| AppError::Io {
                message: format!("Failed to read metadata.json: {}", e),
                kind: format!("{:?}", e.kind()),
            })?;

        let metadata: ArticleMetadata = serde_json::from_str(&content).map_err(|e| {
            AppError::Serialization(format!("Failed to parse metadata.json: {}", e))
        })?;

        Ok(metadata.title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_is_web_article_valid() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("article.md");
        let html_path = temp_dir.path().join("page.html");

        fs::write(&article_path, "content").unwrap();
        fs::write(&html_path, "<html></html>").unwrap();

        assert!(WebArticleDetector::is_web_article(&article_path));
    }

    #[test]
    fn test_is_web_article_missing_html() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("article.md");
        fs::write(&article_path, "content").unwrap();

        assert!(!WebArticleDetector::is_web_article(&article_path));
    }

    #[test]
    fn test_is_web_article_wrong_name() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("other.md");
        let html_path = temp_dir.path().join("page.html");

        fs::write(&article_path, "content").unwrap();
        fs::write(&html_path, "<html></html>").unwrap();

        assert!(!WebArticleDetector::is_web_article(&article_path));
    }

    #[test]
    fn test_is_web_article_weblink_md_supported() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("weblink.md");
        let html_path = temp_dir.path().join("page.html");

        fs::write(&article_path, "content").unwrap();
        fs::write(&html_path, "<html></html>").unwrap();

        assert!(WebArticleDetector::is_web_article(&article_path));
    }

    #[test]
    fn test_get_html_path_exists() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("article.md");
        let html_path = temp_dir.path().join("page.html");

        fs::write(&article_path, "content").unwrap();
        fs::write(&html_path, "<html></html>").unwrap();

        let result = WebArticleDetector::get_html_path(&article_path);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), html_path);
    }

    #[test]
    fn test_get_html_path_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("article.md");
        fs::write(&article_path, "content").unwrap();

        let result = WebArticleDetector::get_html_path(&article_path);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_title_success() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("article.md");
        let metadata_path = temp_dir.path().join("metadata.json");

        fs::write(&article_path, "content").unwrap();
        fs::write(
            &metadata_path,
            r#"{"title": "Test Article", "url": "https://example.com"}"#,
        )
        .unwrap();

        let title = WebArticleDetector::get_title(&article_path).await.unwrap();
        assert_eq!(title, "Test Article");
    }

    #[tokio::test]
    async fn test_get_title_missing_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("article.md");
        fs::write(&article_path, "content").unwrap();

        let result = WebArticleDetector::get_title(&article_path).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_get_title_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("article.md");
        let metadata_path = temp_dir.path().join("metadata.json");

        fs::write(&article_path, "content").unwrap();
        fs::write(&metadata_path, "invalid json").unwrap();

        let result = WebArticleDetector::get_title(&article_path).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::Serialization(_)));
    }

    #[tokio::test]
    async fn test_get_title_with_site_name() {
        let temp_dir = TempDir::new().unwrap();
        let article_path = temp_dir.path().join("article.md");
        let metadata_path = temp_dir.path().join("metadata.json");

        fs::write(&article_path, "content").unwrap();
        fs::write(
            &metadata_path,
            r#"{"title": "Article Title", "url": "https://example.com", "site_name": "Example Site"}"#,
        )
        .unwrap();

        let title = WebArticleDetector::get_title(&article_path).await.unwrap();
        assert_eq!(title, "Article Title");
    }
}
