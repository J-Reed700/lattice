use async_trait::async_trait;
use std::path::{Path, PathBuf};

use lattice_core::application::contracts::web::CleanArticle;
use lattice_core::domain::ports::article_archive::ArticleArchive;
use lattice_core::shared::error::Result;

/// Filesystem-based archival of web articles as markdown files.
#[async_trait]
pub trait WebArchiveServiceTrait: ArticleArchive {
    /// Save web article to archive as a markdown file with metadata frontmatter.
    async fn archive_article(&self, article: CleanArticle, url: &str) -> Result<PathBuf>;

    /// Load article from archive by file path.
    async fn load_article(&self, path: &Path) -> Result<CleanArticle>;

    /// List all article markdown paths in the archive.
    async fn list_articles(&self) -> Result<Vec<PathBuf>>;
}
