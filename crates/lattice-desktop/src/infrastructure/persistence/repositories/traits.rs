//! Repository trait definitions for dependency injection.

use async_trait::async_trait;
use lattice_core::domain::entities::Document;
use lattice_core::shared::error::Result;

/// Trait for document storage operations.
#[async_trait]
pub trait DocumentRepositoryTrait: Send + Sync {
    async fn create(
        &self,
        file_path: &str,
        file_name: &str,
        mime_type: &str,
        size_bytes: i64,
        modified_at: &str,
        checksum: &str,
    ) -> Result<Document>;

    async fn upsert(
        &self,
        file_path: &str,
        file_name: &str,
        mime_type: &str,
        size_bytes: i64,
        modified_at: &str,
        checksum: &str,
    ) -> Result<String>;

    async fn find_by_id(&self, id: &str) -> Result<Option<Document>>;

    async fn find_by_path(&self, file_path: &str) -> Result<Option<Document>>;

    async fn list_all(&self) -> Result<Vec<Document>>;

    async fn update_status(&self, id: &str, status: &str) -> Result<()>;

    async fn delete(&self, id: &str) -> Result<()>;

    async fn count(&self) -> Result<i64>;

    async fn exists(&self, file_path: &str) -> Result<bool>;

    async fn find_by_path_pattern(&self, pattern: &str) -> Result<Vec<Document>>;

    async fn get_by_id(&self, id: &str) -> Result<Option<Document>> {
        self.find_by_id(id).await
    }

    async fn get_all_documents(&self) -> Result<Vec<Document>> {
        self.list_all().await
    }
}
