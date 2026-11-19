//! Document Repository Implementation
//!
//! SQLite pool-based implementation of DocumentRepository that delegates to ops.rs.

use async_trait::async_trait;
use lattice_core::application::ports::{DocumentRepositoryPort, RepositoryPort};
use lattice_core::domain::entities::Document as DocumentEntity;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, Page};

use super::ops;

#[derive(Debug, Clone)]
pub struct SqliteDocumentRepository {
    pub(super) db: Database,
}

impl SqliteDocumentRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn find_by_path(&self, file_path: &str) -> Result<Option<DocumentEntity>> {
        let mut read = self.db.begin_read().await?;
        let result = ops::find_by_path(&mut read, file_path).await?;
        read.commit().await?;
        Ok(result)
    }

    pub async fn find_aggregate_by_path_exact(
        &self,
        file_path: &str,
    ) -> Result<Option<DocumentEntity>> {
        let mut tx = self.db.begin_read().await?;
        let entity = ops::find_by_path(&mut tx, file_path).await?;
        let aggregate = match entity {
            Some(doc_entity) => {
                let chunks =
                    ops::fetch_chunks_for_document(&mut tx, doc_entity.id().as_str()).await?;
                let tags = ops::fetch_tags_for_document(&mut tx, doc_entity.id().as_str()).await?;
                Some(doc_entity.from_parts(chunks, tags)?)
            }
            None => None,
        };
        tx.commit().await?;
        Ok(aggregate)
    }

    pub async fn find_by_path_pattern(&self, pattern: &str) -> Result<Vec<DocumentEntity>> {
        let mut read = self.db.begin_read().await?;
        let result = ops::find_by_path_pattern(&mut read, pattern).await?;
        read.commit().await?;
        Ok(result)
    }

    pub async fn exists_by_path(&self, file_path: &str) -> Result<bool> {
        let mut read = self.db.begin_read().await?;
        let result = ops::exists_by_path(&mut read, file_path).await?;
        read.commit().await?;
        Ok(result)
    }

    pub async fn count_documents(&self) -> Result<i64> {
        let mut read = self.db.begin_read().await?;
        let result = ops::count_documents(&mut read).await?;
        read.commit().await?;
        Ok(result)
    }

    pub async fn count_chunks(&self) -> Result<i64> {
        let mut read = self.db.begin_read().await?;
        let result = ops::count_chunks(&mut read).await?;
        read.commit().await?;
        Ok(result)
    }
}

#[async_trait]
impl RepositoryPort<DocumentEntity> for SqliteDocumentRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<DocumentEntity>> {
        let mut tx = self.db.begin_read().await?;
        let result = ops::find_by_id(&mut tx, id).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn find_all(&self) -> Result<Vec<DocumentEntity>> {
        let mut tx = self.db.begin_read().await?;
        let result = ops::find_all(&mut tx).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn save(&self, entity: &DocumentEntity) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        ops::save(&mut tx, entity).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn save_batch(&self, entities: &[DocumentEntity]) -> Result<()> {
        if entities.is_empty() {
            return Ok(());
        }

        let mut tx = self
            .db
            .begin_write()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        ops::save_batch(&mut tx, entities).await?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        ops::delete(&mut tx, id).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let mut tx = self
            .db
            .begin_write()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        ops::delete_batch_optimized(&mut tx, ids).await?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        let mut read = self.db.begin_read().await?;
        let result = ops::count(&mut read).await?;
        read.commit().await?;
        Ok(result)
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        let mut read = self.db.begin_read().await?;
        let result = ops::exists(&mut read, id).await?;
        read.commit().await?;
        Ok(result)
    }
}

#[async_trait]
impl DocumentRepositoryPort for SqliteDocumentRepository {
    async fn record_accesses(&self, document_ids: &[String]) -> Result<()> {
        if document_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.begin_write().await?;
        ops::record_accesses(&mut tx, document_ids).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn list_metadata(&self) -> Result<Vec<DocumentEntity>> {
        let mut read = self.db.begin_read().await?;
        let result = ops::list_metadata(&mut read).await?;
        read.commit().await?;
        Ok(result)
    }
    async fn find_metadata_by_ids(&self, ids: &[String]) -> Result<Vec<DocumentEntity>> {
        let mut read = self.db.begin_read().await?;
        let result = ops::find_metadata_by_ids(&mut read, ids).await?;
        read.commit().await?;
        Ok(result)
    }
    async fn rename(&self, id: &str, name: &str) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        ops::rename(&mut tx, id, name).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn find_file_path_by_id(&self, document_id: &str) -> Result<String> {
        let mut read = self.db.begin_read().await?;
        let result = ops::find_file_path_by_id(&mut read, document_id).await?;
        read.commit().await?;
        Ok(result)
    }

    async fn document_exists(&self, document_id: &str) -> Result<bool> {
        let mut read = self.db.begin_read().await?;
        let result = ops::exists(&mut read, document_id).await?;
        read.commit().await?;
        Ok(result)
    }

    async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>> {
        let mut read = self.db.begin_read().await?;
        let result = ops::find_id_by_path(&mut read, file_path).await?;
        read.commit().await?;
        Ok(result)
    }

    async fn delete(&self, document_id: &str) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        ops::delete(&mut tx, document_id).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn delete_and_release_blob(&self, document_id: &str) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        ops::delete_and_release_blob(&mut tx, document_id).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn find_by_checksum(
        &self,
        checksum: &lattice_core::domain::value_objects::Checksum,
    ) -> Result<Option<lattice_core::domain::entities::Document>> {
        let mut tx = self.db.begin_read().await?;
        let result = ops::find_aggregate_by_checksum_tx(&mut tx, checksum.as_str()).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn count_documents(&self) -> Result<i64> {
        let mut read = self.db.begin_read().await?;
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&mut *read)
            .await?;
        read.commit().await?;
        Ok(count.0)
    }

    async fn count_chunks(&self) -> Result<i64> {
        let mut read = self.db.begin_read().await?;
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM text_chunks")
            .fetch_one(&mut *read)
            .await?;
        read.commit().await?;
        Ok(count.0)
    }

    async fn count_by_checksum(&self, checksum: &str) -> Result<u64> {
        let mut read = self.db.begin_read().await?;
        let result = ops::count_by_checksum(&mut read, checksum).await?;
        read.commit().await?;
        Ok(result)
    }

    async fn list_checksums(&self) -> Result<Vec<String>> {
        let mut read = self.db.begin_read().await?;
        let result = ops::list_checksums(&mut read).await?;
        read.commit().await?;
        Ok(result)
    }

    async fn find_checksum_by_id(&self, document_id: &str) -> Result<String> {
        let mut read = self.db.begin_read().await?;
        let result = ops::find_checksum_by_id(&mut read, document_id).await?;
        read.commit().await?;
        Ok(result)
    }

    async fn find_all_paginated(&self, page: Page) -> Result<Vec<DocumentEntity>> {
        let mut read = self.db.begin_read().await?;
        let result = ops::find_all_paginated(&mut read, page).await?;
        read.commit().await?;
        Ok(result)
    }
}

impl lattice_core::application::ports::DocumentRepository for SqliteDocumentRepository {}
