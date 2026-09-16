//! Web archive service trait
//!
//! Manages markdown-based web article archives on the local filesystem.
//!
//! ## Architecture
//!
//! Following the "bricks and studs" philosophy:
//! - **Stud (Public Interface)**: WebArchiveServiceTrait defines archive operations
//! - **Brick (Implementation)**: WebArchiveService manages ~/.lattice/web-archive
//! - **Regeneratable**: Can swap storage backend (local files, cloud sync, etc.)
//!
//! ## Security
//!
//! Implementations MUST:
//! - Validate all file paths to prevent directory traversal (CWE-22)
//! - Use ValidatedFilePath for all file operations
//! - Handle file I/O errors gracefully
//! - Create directories with appropriate permissions

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::features::function_calling::dto::{CleanArticle, UrlPreview};
use crate::shared::error::Result;

/// Trait for web archive operations
///
/// Provides filesystem-based archival of web articles as markdown files.
///
/// # Features
///
/// - **Save Article**: Converts CleanArticle to markdown with YAML frontmatter
/// - **Load Article**: Parses markdown file back to CleanArticle
/// - **Delete Article**: Removes article from archive
/// - **Directory Organization**: Organizes articles by domain (e.g., github.com/)
/// - **README Generation**: Creates helpful README explaining the archive
///
/// # Archive Structure
///
/// ```text
/// ~/.lattice/web-archive/
/// ├── README.md
/// ├── github.com/
/// │   ├── article-title-uuid.md
/// │   └── another-article-uuid.md
/// ├── arxiv.org/
/// │   └── paper-title-uuid.md
/// └── example.com/
///     └── blog-post-uuid.md
/// ```
///
/// # Markdown Format
///
/// Each article is stored as markdown with YAML frontmatter:
///
/// ```markdown
/// ---
/// title: "Article Title"
/// author: "Author Name"
/// url: "https://example.com/article"
/// published_date: "2024-01-15T10:30:00Z"
/// word_count: 1500
/// reading_time_minutes: 8
/// archived_at: "2024-01-20T14:22:00Z"
/// ---
///
/// # Article Title
///
/// Article content here...
/// ```
///
/// # Implementations
/// - `WebArchiveService`: Production implementation with tokio::fs
/// - `MockWebArchiveService`: In-memory mock for testing
#[async_trait]
pub trait WebArchiveServiceTrait: Send + Sync {
    /// Save web article to archive as markdown file
    ///
    /// Converts the article to markdown with YAML frontmatter and saves it
    /// to the appropriate domain subdirectory. Creates subdirectories as needed.
    ///
    /// # Arguments
    /// * `article` - The article to archive
    /// * `url` - Source URL for organization and metadata
    ///
    /// # Returns
    /// `PathBuf` pointing to the created markdown file
    ///
    /// # Errors
    /// - `AppError::Io` if file writing fails
    /// - `AppError::InvalidInput` if article data is invalid
    /// - `AppError::Other` if directory creation fails
    ///
    /// # Example
    /// ```rust,no_run
    /// use lattice::infrastructure::services::traits::WebArchiveServiceTrait;
    /// use lattice::application::dtos::function_calling_dto::CleanArticle;
    ///
    /// # async fn example<S: WebArchiveServiceTrait>(service: &S) -> Result<(), Box<dyn std::error::Error>> {
    /// let article = CleanArticle {
    ///     title: "Rust Best Practices".to_string(),
    ///     author: Some("Ferris".to_string()),
    ///     content: "<p>Content here...</p>".to_string(),
    ///     text_content: "Content here...".to_string(),
    ///     word_count: 1500,
    ///     reading_time_minutes: 8,
    ///     published_date: None,
    ///     excerpt: Some("Brief summary".to_string()),
    /// };
    ///
    /// let path = service.archive_article(article, "https://example.com/article").await?;
    /// println!("Archived to: {}", path.display());
    /// # Ok(())
    /// # }
    /// ```
    async fn archive_article(&self, article: CleanArticle, url: &str) -> Result<PathBuf>;

    /// Load article from archive by file path
    ///
    /// Reads and parses a markdown file from the archive, extracting both
    /// the YAML frontmatter and markdown content back into a CleanArticle.
    ///
    /// # Arguments
    /// * `path` - Path to the markdown file in the archive
    ///
    /// # Returns
    /// `CleanArticle` reconstructed from the markdown file
    ///
    /// # Errors
    /// - `AppError::NotFound` if file doesn't exist
    /// - `AppError::Io` if file reading fails
    /// - `AppError::InvalidFormat` if markdown parsing fails
    /// - `AppError::Other` if frontmatter is malformed
    ///
    /// # Example
    /// ```rust,no_run
    /// use lattice::infrastructure::services::traits::WebArchiveServiceTrait;
    /// use std::path::PathBuf;
    ///
    /// # async fn example<S: WebArchiveServiceTrait>(service: &S) -> Result<(), Box<dyn std::error::Error>> {
    /// let path = PathBuf::from("/Users/user/.lattice/web-archive/example.com/article.md");
    /// let article = service.load_article(&path).await?;
    /// println!("Title: {}", article.title);
    /// # Ok(())
    /// # }
    /// ```
    async fn load_article(&self, path: &Path) -> Result<CleanArticle>;

    /// Delete article from archive
    ///
    /// Removes the markdown file from the archive. Does not fail if file
    /// doesn't exist (idempotent operation).
    ///
    /// # Arguments
    /// * `path` - Path to the markdown file to delete
    ///
    /// # Returns
    /// `Ok(())` if file was deleted or didn't exist
    ///
    /// # Errors
    /// - `AppError::Io` if file deletion fails due to permissions or other I/O error
    ///
    /// # Example
    /// ```rust,no_run
    /// use lattice::infrastructure::services::traits::WebArchiveServiceTrait;
    /// use std::path::PathBuf;
    ///
    /// # async fn example<S: WebArchiveServiceTrait>(service: &S) -> Result<(), Box<dyn std::error::Error>> {
    /// let path = PathBuf::from("/Users/user/.lattice/web-archive/example.com/article.md");
    /// service.delete_article(&path).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn delete_article(&self, path: &Path) -> Result<()>;

    /// List all articles in the archive
    ///
    /// Returns paths to all markdown files in the archive directory tree.
    /// Excludes README.md files.
    ///
    /// # Returns
    /// Vector of `PathBuf` for each article file
    ///
    /// # Errors
    /// - `AppError::Io` if directory traversal fails
    ///
    /// # Example
    /// ```rust,no_run
    /// use lattice::infrastructure::services::traits::WebArchiveServiceTrait;
    ///
    /// # async fn example<S: WebArchiveServiceTrait>(service: &S) -> Result<(), Box<dyn std::error::Error>> {
    /// let articles = service.list_articles().await?;
    /// println!("Found {} archived articles", articles.len());
    /// # Ok(())
    /// # }
    /// ```
    async fn list_articles(&self) -> Result<Vec<PathBuf>>;
}
