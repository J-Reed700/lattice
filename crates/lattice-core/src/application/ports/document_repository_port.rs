//! Document repository port for document-specific queries and metadata lookups.

use crate::application::ports::repository_port::RepositoryPort;
use crate::domain::entities::Document;
use crate::shared::error::Result;
use async_trait::async_trait;

/// Port for document-specific repository operations.
#[async_trait]
pub trait DocumentRepositoryPort: RepositoryPort<Document> {
    /// List document metadata without requiring indexed chunks or loading document bodies.
    /// Pending and failed documents remain visible in the inventory.
    async fn list_metadata(&self) -> Result<Vec<Document>> {
        self.find_all().await
    }

    /// Metadata for just these documents, in no particular order; unknown ids are left out.
    async fn find_metadata_by_ids(&self, ids: &[String]) -> Result<Vec<Document>> {
        let wanted: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        Ok(self
            .list_metadata()
            .await?
            .into_iter()
            .filter(|doc| wanted.contains(doc.id().as_str()))
            .collect())
    }

    /// Find the file path for a document by its ID.
    async fn find_file_path_by_id(&self, document_id: &str) -> Result<String>;

    /// Rename a document, touching only its metadata.
    ///
    /// Deliberately narrow. Renaming through the full aggregate `save()` path
    /// rewrites the document's children: the aggregate reconstructed for a
    /// rename carries no chunks, and the save deletes every existing chunk
    /// before inserting that empty set, cascading the embeddings away. The
    /// document then can't be loaded ("must have at least one chunk") and has
    /// silently vanished from search. A metadata-only edit must not go
    /// anywhere near child rows.
    async fn rename(&self, _document_id: &str, _new_file_name: &str) -> Result<()> {
        Err(crate::shared::error::AppError::Other(
            "rename is not supported by this repository implementation".to_string(),
        ))
    }

    /// Check if a document exists by ID.
    async fn document_exists(&self, document_id: &str) -> Result<bool>;

    /// Find document ID by file path.
    async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>>;

    /// Delete a document by ID.
    async fn delete(&self, document_id: &str) -> Result<()>;

    /// Find a document by its content checksum.
    async fn find_by_checksum(
        &self,
        checksum: &crate::domain::value_objects::Checksum,
    ) -> Result<Option<Document>>;

    /// Count total number of indexed documents.
    async fn count_documents(&self) -> Result<i64>;

    /// Count total number of chunks across all documents.
    async fn count_chunks(&self) -> Result<i64>;

    /// How many documents reference this content checksum (used for blob garbage collection).
    async fn count_by_checksum(&self, checksum: &str) -> Result<u64> {
        let checksum = crate::domain::value_objects::Checksum::new(checksum.to_string())?;
        Ok(u64::from(self.find_by_checksum(&checksum).await?.is_some()))
    }

    /// Every distinct content checksum referenced by a document.
    async fn list_checksums(&self) -> Result<Vec<String>> {
        let mut checksums: Vec<String> = self
            .find_all()
            .await?
            .iter()
            .map(|document| document.checksum().as_str().to_string())
            .collect();
        checksums.sort();
        checksums.dedup();
        Ok(checksums)
    }

    /// Content checksum of one document, without loading chunks or bodies.
    async fn find_checksum_by_id(&self, document_id: &str) -> Result<String> {
        self.find_by_id(document_id)
            .await?
            .map(|document| document.checksum().as_str().to_string())
            .ok_or_else(|| {
                crate::shared::error::AppError::NotFound(format!(
                    "Document not found: {document_id}"
                ))
            })
    }

    /// Find documents with a SQL-level LIMIT applied, ordered by `indexed_at DESC`.
    async fn find_all_paginated(&self, limit: usize) -> Result<Vec<Document>>;
}
