//! In-memory `MockChunkRepository`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::application::ports::{ChunkRepositoryPort, RepositoryPort};
use crate::domain::entities::chunk::Chunk as ChunkEntity;
use crate::domain::entities::Chunk;
use crate::shared::error::Result;

#[derive(Clone)]
pub struct MockChunkRepository {
    chunks: Arc<RwLock<HashMap<String, Chunk>>>,
    document_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl MockChunkRepository {
    pub fn new() -> Self {
        Self {
            chunks: Arc::new(RwLock::new(HashMap::new())),
            document_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn clear(&self) {
        self.chunks.write().clear();
        self.document_index.write().clear();
    }
}

impl Default for MockChunkRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChunkRepositoryPort for MockChunkRepository {
    async fn create(
        &self,
        document_id: &str,
        content: &str,
        _context_prefix: Option<&str>,
        _contextualized_content: Option<&str>,
        index: usize,
        _start_char: Option<i64>,
        _end_char: Option<i64>,
    ) -> Result<Chunk> {
        use crate::shared::types::DocumentId;
        let doc_id = DocumentId::from(document_id.to_string());
        let chunk = Chunk::new(doc_id, content.to_string(), index);
        self.save(&chunk).await?;
        Ok(chunk)
    }

    async fn find_by_document(&self, document_id: &str) -> Result<Vec<Chunk>> {
        let document_index = self.document_index.read();
        let chunk_ids = document_index.get(document_id);

        if let Some(ids) = chunk_ids {
            let chunks_map = self.chunks.read();
            let mut chunks: Vec<Chunk> = ids
                .iter()
                .filter_map(|id| chunks_map.get(id).cloned())
                .collect();
            chunks.sort_by_key(|c| c.chunk_index());
            Ok(chunks)
        } else {
            Ok(Vec::new())
        }
    }

    async fn find_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<Chunk>> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }

        let chunks_map = self.chunks.read();
        let chunks = chunk_ids
            .iter()
            .filter_map(|id| chunks_map.get(id).cloned())
            .collect();

        Ok(chunks)
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        let mut document_index = self.document_index.write();
        if let Some(chunk_ids) = document_index.remove(document_id) {
            let mut chunks = self.chunks.write();
            for id in chunk_ids {
                chunks.remove(&id);
            }
        }
        Ok(())
    }

    async fn count_all(&self) -> Result<i64> {
        Ok(self.chunks.read().len() as i64)
    }

    async fn count_indexed_documents(&self) -> Result<i64> {
        Ok(self.document_index.read().len() as i64)
    }

    async fn count_by_document(&self, document_id: &str) -> Result<i64> {
        let document_index = self.document_index.read();
        Ok(document_index
            .get(document_id)
            .map(|ids| ids.len() as i64)
            .unwrap_or(0))
    }

    async fn create_batch(&self, chunks: Vec<Chunk>) -> Result<Vec<Chunk>> {
        for chunk in &chunks {
            self.save(chunk).await?;
        }
        Ok(chunks)
    }
}

#[async_trait]
impl RepositoryPort<ChunkEntity> for MockChunkRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<ChunkEntity>> {
        Ok(self.chunks.read().get(id).cloned())
    }

    async fn find_all(&self) -> Result<Vec<ChunkEntity>> {
        Ok(self.chunks.read().values().cloned().collect())
    }

    async fn save(&self, entity: &ChunkEntity) -> Result<()> {
        let id = entity.id().as_str().to_string();
        let doc_id = entity.document_id().as_str().to_string();

        self.chunks.write().insert(id.clone(), entity.clone());

        let mut doc_index = self.document_index.write();
        doc_index.entry(doc_id).or_default().push(id);

        Ok(())
    }

    async fn save_batch(&self, entities: &[ChunkEntity]) -> Result<()> {
        for entity in entities {
            self.save(entity).await?;
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        if let Some(chunk) = self.chunks.write().remove(id) {
            let doc_id = chunk.document_id().as_str();
            let mut doc_index = self.document_index.write();
            if let Some(chunks) = doc_index.get_mut(doc_id) {
                chunks.retain(|chunk_id| chunk_id != id);
            }
        }
        Ok(())
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        for id in ids {
            <Self as RepositoryPort<ChunkEntity>>::delete(self, id).await?;
        }
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.chunks.read().len())
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.chunks.read().contains_key(id))
    }

    async fn find_by_filter(
        &self,
        _filter: &dyn crate::application::ports::Filter,
    ) -> Result<Vec<ChunkEntity>> {
        // For mock, just return all chunks
        self.find_all().await
    }
}
// END REMOVED MockChunkRepository
