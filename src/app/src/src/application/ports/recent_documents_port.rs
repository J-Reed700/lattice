//! Recent Documents Repository Port

use crate::application::contracts::recent_documents::RecentDocumentRecord;
use crate::shared::result::Result;
use async_trait::async_trait;

#[async_trait]
pub trait RecentDocumentsRepositoryPort: Send + Sync {
    async fn track_access(&self, document_id: &str) -> Result<()>;

    async fn get_recent_documents(&self, limit: usize) -> Result<Vec<RecentDocumentRecord>>;

    async fn clear_recent_history(&self, before_date: Option<&str>) -> Result<usize>;
}
