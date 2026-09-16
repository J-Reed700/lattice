//! Document Repository Transaction Wrapper
//!
//! Transaction-based implementation of DocumentRepository that delegates to ops.rs.
//! This enables transactional operations within a broader unit of work.

use crate::application::ports::{DocumentRepositoryPort, Filter, RepositoryPort};
use crate::domain::entities::Document as DocumentEntity;
use crate::infrastructure::persistence::mappers::DocumentModel;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::{Sqlite, Transaction};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

use super::ops;

pub struct SqliteDocumentRepositoryTx {
    transaction: Weak<Mutex<Transaction<'static, Sqlite>>>,
}

impl SqliteDocumentRepositoryTx {
    pub fn new(transaction: Arc<Mutex<Transaction<'static, Sqlite>>>) -> Self {
        Self {
            transaction: Arc::downgrade(&transaction),
        }
    }

    fn get_transaction(&self) -> Result<Arc<Mutex<Transaction<'static, Sqlite>>>> {
        self.transaction
            .upgrade()
            .ok_or_else(|| AppError::InternalError("Transaction dropped".into()))
    }

    pub async fn find_by_path(&self, file_path: &str) -> Result<Option<DocumentEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_path(&mut tx, file_path).await
    }

    pub async fn find_aggregate_by_path_exact(
        &self,
        file_path: &str,
    ) -> Result<Option<DocumentEntity>> {
        let entity = self.find_by_path(file_path).await?;

        match entity {
            Some(doc_entity) => {
                let tx_arc = self.get_transaction()?;
                let mut tx = tx_arc.lock().await;
                let chunks =
                    ops::fetch_chunks_for_document(&mut tx, doc_entity.id().as_str()).await?;
                let tags = ops::fetch_tags_for_document(&mut tx, doc_entity.id().as_str()).await?;
                let aggregate = doc_entity.from_parts(chunks, tags)?;
                Ok(Some(aggregate))
            }
            None => Ok(None),
        }
    }

    pub async fn find_by_path_pattern(&self, pattern: &str) -> Result<Vec<DocumentEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_path_pattern(&mut tx, pattern).await
    }

    pub async fn exists_by_path(&self, file_path: &str) -> Result<bool> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::exists_by_path(&mut tx, file_path).await
    }

    pub async fn count_documents(&self) -> Result<i64> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count_documents(&mut tx).await
    }

    pub async fn count_chunks(&self) -> Result<i64> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count_chunks(&mut tx).await
    }
}

#[async_trait]
impl RepositoryPort<DocumentEntity> for SqliteDocumentRepositoryTx {
    async fn find_by_id(&self, id: &str) -> Result<Option<DocumentEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_id(&mut tx, id).await
    }

    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<DocumentEntity>> {
        filter.validate()?;

        let filter_any = filter.as_any();
        if let Some(doc_filter) = filter_any.downcast_ref::<super::implementation::DocumentFilter>()
        {
            let path_pattern = doc_filter.path_pattern.clone();
            let status = doc_filter.status.clone();
            let limit = doc_filter.limit.unwrap_or(1000);

            let mut query = String::from(
                "SELECT id, file_path, file_name, file_type, mime_type, size_bytes,
                 modified_at, indexed_at, checksum, status, language, category,
                 quality_score, access_count, last_accessed_at, word_count FROM documents WHERE 1=1",
            );

            if path_pattern.is_some() {
                query.push_str(" AND file_path LIKE ?");
            }
            if status.is_some() {
                query.push_str(" AND status = ?");
            }
            query.push_str(" ORDER BY indexed_at DESC LIMIT ?");

            let mut query_builder = sqlx::query_as::<_, DocumentModel>(&query);

            if let Some(pattern) = &path_pattern {
                query_builder = query_builder.bind(pattern);
            }
            if let Some(s) = &status {
                query_builder = query_builder.bind(s);
            }
            query_builder = query_builder.bind(limit as i64);

            let tx_arc = self.get_transaction()?;
            let mut tx = tx_arc.lock().await;
            let db_models = query_builder.fetch_all(&mut **tx).await.map_err(|e| {
                AppError::Database(format!("Failed to find documents by filter: {}", e))
            })?;

            Ok(
                crate::infrastructure::persistence::mappers::DocumentMapper::to_entities(
                    &db_models,
                ),
            )
        } else {
            let tx_arc = self.get_transaction()?;
            let mut tx = tx_arc.lock().await;
            ops::find_all(&mut tx).await
        }
    }

    async fn find_all(&self) -> Result<Vec<DocumentEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_all(&mut tx).await
    }

    async fn save(&self, entity: &DocumentEntity) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::save(&mut tx, entity).await
    }

    async fn save_batch(&self, entities: &[DocumentEntity]) -> Result<()> {
        if entities.is_empty() {
            return Ok(());
        }

        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::save_batch_optimized(&mut tx, entities).await?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::delete(&mut tx, id).await
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::delete_batch_optimized(&mut tx, ids).await?;

        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count(&mut tx).await
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::exists(&mut tx, id).await
    }
}

#[async_trait]
impl DocumentRepositoryPort for SqliteDocumentRepositoryTx {
    async fn find_file_path_by_id(&self, document_id: &str) -> Result<String> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_file_path_by_id(&mut tx, document_id).await
    }

    async fn document_exists(&self, document_id: &str) -> Result<bool> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::exists(&mut tx, document_id).await
    }

    async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_id_by_path(&mut tx, file_path).await
    }

    async fn delete(&self, document_id: &str) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::delete(&mut tx, document_id).await
    }

    async fn find_by_checksum(
        &self,
        checksum: &crate::domain::value_objects::Checksum,
    ) -> Result<Option<crate::domain::entities::Document>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_checksum(&mut tx, checksum).await
    }

    async fn count_documents(&self) -> Result<i64> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&mut **tx)
            .await?;
        Ok(count.0)
    }

    async fn count_chunks(&self) -> Result<i64> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM text_chunks")
            .fetch_one(&mut **tx)
            .await?;
        Ok(count.0)
    }

    async fn find_all_paginated(&self, limit: usize) -> Result<Vec<DocumentEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_all_paginated(&mut tx, limit).await
    }
}

impl crate::application::ports::DocumentRepository for SqliteDocumentRepositoryTx {}
