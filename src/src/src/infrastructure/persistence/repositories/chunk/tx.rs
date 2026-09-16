//! Chunk Repository Transaction Wrapper
//!
//! Transaction-based implementation of ChunkRepository that delegates to ops.rs.
//! This enables transactional operations within a broader unit of work.

use crate::application::ports::{ChunkRepositoryPort, Filter, RepositoryPort};
use crate::domain::entities::chunk::Chunk as ChunkEntity;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::{Sqlite, Transaction};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

use super::ops;

pub struct SqliteChunkRepositoryTx {
    transaction: Weak<Mutex<Transaction<'static, Sqlite>>>,
}

impl SqliteChunkRepositoryTx {
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

    pub async fn find_by_document(&self, document_id: &str) -> Result<Vec<ChunkEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_document(&mut tx, document_id).await
    }

    pub async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::delete_by_document(&mut tx, document_id).await
    }

    pub async fn count_by_document(&self, document_id: &str) -> Result<usize> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count_by_document(&mut tx, document_id).await
    }
}

#[async_trait]
impl RepositoryPort<ChunkEntity> for SqliteChunkRepositoryTx {
    async fn find_by_id(&self, id: &str) -> Result<Option<ChunkEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_id(&mut tx, id).await
    }

    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<ChunkEntity>> {
        filter.validate()?;

        let filter_any = filter.as_any();
        if let Some(chunk_filter) = filter_any.downcast_ref::<super::implementation::ChunkFilter>()
        {
            if let Some(document_id) = &chunk_filter.document_id {
                let tx_arc = self.get_transaction()?;
                let mut tx = tx_arc.lock().await;
                return ops::find_by_document(&mut tx, document_id).await;
            }
        }

        Ok(vec![])
    }

    async fn find_all(&self) -> Result<Vec<ChunkEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_all(&mut tx).await
    }

    async fn save(&self, entity: &ChunkEntity) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::save(&mut tx, entity).await
    }

    async fn save_batch(&self, entities: &[ChunkEntity]) -> Result<()> {
        if entities.is_empty() {
            return Ok(());
        }

        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        for entity in entities {
            ops::save(&mut tx, entity).await?;
        }

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
        for id in ids {
            ops::delete(&mut tx, id).await?;
        }

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
impl ChunkRepositoryPort for SqliteChunkRepositoryTx {
    async fn create(
        &self,
        document_id: &str,
        content: &str,
        _context_prefix: Option<&str>,
        _contextualized_content: Option<&str>,
        index: usize,
        _start_char: Option<i64>,
        _end_char: Option<i64>,
    ) -> Result<ChunkEntity> {
        use crate::shared::domain_types::DocumentId;
        let doc_id = DocumentId::from(document_id.to_string());
        let chunk = ChunkEntity::new(doc_id, content.to_string(), index);
        RepositoryPort::save(self, &chunk).await?;
        Ok(chunk)
    }

    async fn find_by_document(&self, document_id: &str) -> Result<Vec<ChunkEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_document(&mut tx, document_id).await
    }

    async fn find_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<ChunkEntity>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_ids(&mut tx, chunk_ids).await
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::delete_by_document(&mut tx, document_id).await
    }

    async fn count_all(&self) -> Result<i64> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count_all(&mut tx).await
    }

    async fn count_indexed_documents(&self) -> Result<i64> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count_indexed_documents(&mut tx).await
    }

    async fn count_by_document(&self, document_id: &str) -> Result<i64> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count_by_document(&mut tx, document_id)
            .await
            .map(|c| c as i64)
    }

    async fn create_batch(&self, chunks: Vec<ChunkEntity>) -> Result<Vec<ChunkEntity>> {
        RepositoryPort::save_batch(self, &chunks).await?;
        Ok(chunks)
    }
}
