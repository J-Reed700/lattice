//! Article Archive Port
//!
//! Some documents keep their file in an archive the app maintains for them,
//! such as saved web articles. Deleting one of those documents has to go
//! through the archive, and only the archive can say which paths are its own.

use crate::shared::error::Result;
use std::path::Path;

/// The part of an article archive that document deletion needs.
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
}
