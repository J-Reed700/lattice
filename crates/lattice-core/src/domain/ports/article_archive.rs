//! Article Archive Port
//!
//! Some documents keep their file in an archive the app maintains for them,
//! such as saved web articles. The archive is the one place that knows which
//! paths are its own and how an article is laid out, so deleting one of those
//! documents, opening it and naming it all ask the archive.

use crate::shared::error::Result;
use std::path::{Path, PathBuf};

/// A saved web article's files and the facts the archive keeps about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedArticle {
    /// The article's text (`article.md`), the file its document indexes.
    pub text: PathBuf,
    /// The saved page (`page.html`), when it is still on disk.
    pub page: Option<PathBuf>,
    pub title: Option<String>,
    /// The address the article was saved from.
    pub url: Option<String>,
    /// Outgoing links captured from the page.
    pub links: Vec<String>,
}

/// What the rest of the app asks of an article archive: which paths are its
/// own, which of them are saved articles, and how to remove one.
#[async_trait::async_trait]
pub trait ArticleArchive: Send + Sync {
    /// Remove the article at `path`. Succeeds when the file is already gone.
    async fn delete_article(&self, path: &Path) -> Result<()>;

    /// True when `path` is inside this archive.
    ///
    /// Web documents keep the article path in `documents.file_path`, and
    /// deletion has to tell an archived article apart from a library blob or a
    /// file the user keeps somewhere of their own. Only the archive knows
    /// where its own base directory is, so only it can answer.
    fn owns(&self, path: &Path) -> bool;

    /// The archived article whose text file is `path`, or `None` when `path` is
    /// not an article's text file inside this archive.
    async fn saved_article(&self, path: &Path) -> Option<SavedArticle>;
}
