use crate::application::contracts::file_library::{IndexedFolder, IndexingActivity};
use crate::shared::error::Result;

#[async_trait::async_trait]
pub trait FileLibraryPort: Send + Sync {
    async fn indexed_folders(&self) -> Result<Vec<IndexedFolder>>;
    async fn indexing_activities(&self, limit: usize) -> Result<Vec<IndexingActivity>>;
}
