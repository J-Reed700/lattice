//! Embedding Repository Transaction Wrapper
//!
//! Transaction-based implementation of EmbeddingRepository that delegates to ops.rs.
//! This enables transactional operations within a broader unit of work.

use crate::application::ports::EmbeddingRepositoryPort;
use crate::domain::entities::embedding::Embedding as DomainEmbedding;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::{Sqlite, Transaction};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

use super::ops;

pub struct SqliteEmbeddingRepositoryTx {
    pub(crate) transaction: Weak<Mutex<Transaction<'static, Sqlite>>>,
}

impl SqliteEmbeddingRepositoryTx {
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
}

#[async_trait]
impl EmbeddingRepositoryPort for SqliteEmbeddingRepositoryTx {
    async fn create(&self, chunk_id: &str, vector: &[f32], model: &str) -> Result<String> {
        use crate::shared::domain_types::ChunkId;
        let chunk_id_typed = ChunkId::from(chunk_id.to_string());
        let embedding = DomainEmbedding::new(chunk_id_typed, model.to_string(), vector.len());
        self.save(&embedding, vector.to_vec()).await?;
        Ok(embedding.id().to_string())
    }

    async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<DomainEmbedding>> {
        let result = self.find_by_chunk_id(chunk_id).await?;
        Ok(result.map(|(entity, _vector)| entity))
    }

    async fn save(&self, entity: &DomainEmbedding, vector: Vec<f32>) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::save(&mut tx, entity, vector).await
    }

    async fn save_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::save_batch(&mut tx, entries).await
    }

    async fn find_by_chunk_id(
        &self,
        chunk_id: &str,
    ) -> Result<Option<(DomainEmbedding, Vec<f32>)>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_chunk_id(&mut tx, chunk_id).await
    }

    async fn find_by_document_id(
        &self,
        document_id: &str,
    ) -> Result<Vec<(DomainEmbedding, Vec<f32>)>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_document_id(&mut tx, document_id).await
    }

    async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::delete_by_chunk_id(&mut tx, chunk_id).await
    }

    async fn delete_by_document_id(&self, document_id: &str) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::delete_by_document_id(&mut tx, document_id).await
    }

    async fn count(&self) -> Result<i64> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count(&mut tx).await
    }

    async fn create_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<Vec<String>> {
        let ids: Vec<String> = entries.iter().map(|(e, _)| e.id().to_string()).collect();
        self.save_batch(entries).await?;
        Ok(ids)
    }
}
