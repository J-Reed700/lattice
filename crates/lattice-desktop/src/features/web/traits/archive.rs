//! Web archive service trait: web articles archived as markdown files on the
//! local filesystem.

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use lattice_core::application::contracts::web::CleanArticle;
use lattice_core::domain::ports::article_archive::ArticleArchive;
use lattice_core::shared::error::Result;

/// Filesystem archival of web articles as markdown files.
#[async_trait]
pub trait WebArchiveServiceTrait: ArticleArchive {
    /// Saves an article to the archive as markdown with YAML frontmatter, in its
    /// domain's subdirectory.
    ///
    /// # Errors
    /// - `AppError::Io` if file writing fails
    /// - `AppError::InvalidInput` if article data is invalid
    /// - `AppError::Other` if directory creation fails
    async fn archive_article(&self, article: CleanArticle, url: &str) -> Result<PathBuf>;

    /// Loads an archived article from its markdown file.
    ///
    /// # Errors
    /// - `AppError::NotFound` if file doesn't exist
    /// - `AppError::Io` if file reading fails
    /// - `AppError::InvalidFormat` if markdown parsing fails
    /// - `AppError::Other` if frontmatter is malformed
    async fn load_article(&self, path: &Path) -> Result<CleanArticle>;

    /// The paths of every archived article, excluding README.md files.
    ///
    /// # Errors
    /// - `AppError::Io` if directory traversal fails
    async fn list_articles(&self) -> Result<Vec<PathBuf>>;
}
