//! Persistence boundary for assigning imported documents to spaces.

use crate::shared::error::Result;

#[async_trait::async_trait]
pub trait DocumentScopePort: Send + Sync {
    async fn space_exists(&self, id: &str) -> Result<bool>;
    async fn conversation_exists(&self, id: &str) -> Result<bool>;
    /// Assign all documents atomically and idempotently. Empty input is a no-op.
    async fn assign_documents(&self, document_ids: &[String], space_id: &str) -> Result<()>;
}
