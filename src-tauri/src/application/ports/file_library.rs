use crate::application::contracts::file_library::{IndexedFolder, IndexingActivity};
use crate::shared::error::Result;

#[async_trait::async_trait]
pub trait FileLibraryPort: Send + Sync {
    /// Atomically remove the watch entry and associated documents, not disk files.
    /// The caller must validate the path against the file-access policy first.
    async fn remove_folder(&self, path: &str) -> Result<u64>;
    async fn indexed_folders(&self) -> Result<Vec<IndexedFolder>>;
    async fn indexing_activities(&self, limit: usize) -> Result<Vec<IndexingActivity>>;
}
