//! In-memory `MockEmbeddingRepository`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::RwLock;

use lattice_core::application::ports::EmbeddingRepositoryPort;
use lattice_core::shared::error::Result;

use crate::features::embedding::repository::Embedding;
use crate::infrastructure::persistence::repositories::embedding::mapper::{
    EmbeddingDTO, EmbeddingMapper,
};
use lattice_core::domain::entities::Embedding as DomainEmbedding;

/// In-memory mock implementation of EmbeddingRepositoryTrait
#[derive(Clone)]
pub struct MockEmbeddingRepository {
    embeddings: Arc<RwLock<HashMap<String, Embedding>>>,
    chunk_index: Arc<RwLock<HashMap<String, String>>>, // chunk_id -> embedding_id
    chunk_to_document: Arc<RwLock<HashMap<String, String>>>, // chunk_id -> document_id
    document_index: Arc<RwLock<HashMap<String, Vec<String>>>>, // document_id -> Vec<embedding_id>
}

impl MockEmbeddingRepository {
    pub fn new() -> Self {
        Self {
            embeddings: Arc::new(RwLock::new(HashMap::new())),
            chunk_index: Arc::new(RwLock::new(HashMap::new())),
            chunk_to_document: Arc::new(RwLock::new(HashMap::new())),
            document_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register the document that a chunk belongs to for mock queries.
    pub fn register_chunk_document(&self, chunk_id: &str, document_id: &str) {
        self.chunk_to_document
            .write()
            .insert(chunk_id.to_string(), document_id.to_string());
    }

    pub fn clear(&self) {
        self.embeddings.write().clear();
        self.chunk_index.write().clear();
        self.chunk_to_document.write().clear();
        self.document_index.write().clear();
    }
}

impl Default for MockEmbeddingRepository {
    fn default() -> Self {
        Self::new()
    }
}

/*
#[async_trait]
impl EmbeddingRepositoryTrait for MockEmbeddingRepository {
    async fn create(&self, chunk_id: &str, embedding: &[f32], model_name: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let dimension = embedding.len() as i32;

        let emb = Embedding {
            id: id.clone(),
            chunk_id: chunk_id.to_string(),
            embedding: embedding.to_vec(),
            model_name: model_name.to_string(),
            dimension,
        };

        self.embeddings.write().insert(id.clone(), emb);
        self.chunk_index
            .write()
            .insert(chunk_id.to_string(), id.clone());

        if let Some(document_id) = self.chunk_to_document.read().get(chunk_id) {
            self.document_index
                .write()
                .entry(document_id.clone())
                .or_insert_with(Vec::new)
                .push(id.clone());
        }

        Ok(id)
    }

    async fn create_batch(
        &self,
        chunk_embeddings: Vec<(&str, Vec<f32>)>,
        model_name: &str,
    ) -> Result<Vec<String>> {
        let mut embedding_ids = Vec::new();

        for (chunk_id, embedding) in chunk_embeddings {
            let id = self.create(chunk_id, &embedding, model_name).await?;
            embedding_ids.push(id);
        }

        Ok(embedding_ids)
    }

    async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<Embedding>> {
        let chunk_index = self.chunk_index.read();
        if let Some(embedding_id) = chunk_index.get(chunk_id) {
            Ok(self.embeddings.read().get(embedding_id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn find_by_document(&self, document_id: &str) -> Result<Vec<Embedding>> {
        let doc_index = self.document_index.read();
        let embedding_ids = doc_index.get(document_id).cloned().unwrap_or_default();

        let embeddings = self.embeddings.read();
        Ok(embedding_ids
            .iter()
            .filter_map(|id| embeddings.get(id).cloned())
            .collect())
    }

    async fn delete_by_chunk(&self, chunk_id: &str) -> Result<()> {
        let mut chunk_index = self.chunk_index.write();
        if let Some(embedding_id) = chunk_index.remove(chunk_id) {
            self.embeddings.write().remove(&embedding_id);

            // Also remove from document index if tracked
            if let Some(document_id) = self.chunk_to_document.read().get(chunk_id) {
                if let Some(embedding_ids) = self.document_index.write().get_mut(document_id) {
                    embedding_ids.retain(|id| id != &embedding_id);
                }
            }
        }
        Ok(())
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        let mut document_index = self.document_index.write();
        if let Some(embedding_ids) = document_index.remove(document_id) {
            let mut embeddings = self.embeddings.write();
            let mut chunk_index = self.chunk_index.write();

            for embedding_id in embedding_ids {
                if let Some(embedding) = embeddings.remove(&embedding_id) {
                    chunk_index.remove(&embedding.chunk_id);
                }
            }
        }
        Ok(())
    }

    async fn count(&self) -> Result<i64> {
        Ok(self.embeddings.read().len() as i64)
    }
}
*/

#[async_trait]
impl EmbeddingRepositoryPort for MockEmbeddingRepository {
    async fn create(&self, chunk_id: &str, vector: &[f32], model: &str) -> Result<String> {
        use lattice_core::shared::types::ChunkId;
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
        EmbeddingMapper::validate_dimension(entity, &vector)?;

        let dto = EmbeddingMapper::to_dto(entity, vector);
        let embedding_id = uuid::Uuid::new_v4().to_string();

        let embedding = Embedding {
            id: embedding_id.clone(),
            chunk_id: dto.chunk_id.clone(),
            embedding: dto.embedding,
            model_name: dto.model_name,
            created_at: Utc::now().to_rfc3339(),
        };

        self.embeddings
            .write()
            .insert(embedding_id.clone(), embedding);
        self.chunk_index
            .write()
            .insert(dto.chunk_id.clone(), embedding_id.clone());

        if let Some(document_id) = self.chunk_to_document.read().get(&dto.chunk_id) {
            self.document_index
                .write()
                .entry(document_id.clone())
                .or_default()
                .push(embedding_id);
        }

        Ok(())
    }

    async fn save_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<()> {
        for (entity, vector) in entries {
            self.save(&entity, vector).await?;
        }
        Ok(())
    }

    async fn find_by_chunk_id(
        &self,
        chunk_id: &str,
    ) -> Result<Option<(DomainEmbedding, Vec<f32>)>> {
        let chunk_index = self.chunk_index.read();
        if let Some(embedding_id) = chunk_index.get(chunk_id) {
            let embeddings = self.embeddings.read();
            if let Some(emb) = embeddings.get(embedding_id) {
                let dto = EmbeddingDTO {
                    chunk_id: emb.chunk_id.clone(),
                    embedding: emb.embedding.clone(),
                    model_name: emb.model_name.clone(),
                    dimension: emb.embedding.len(), // Derive from vector length
                    computed_at: Utc::now(),
                    model_version: None,
                };

                let entity = EmbeddingMapper::from_dto(&dto)?;
                Ok(Some((entity, emb.embedding.clone())))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    async fn find_by_document_id(
        &self,
        document_id: &str,
    ) -> Result<Vec<(DomainEmbedding, Vec<f32>)>> {
        let document_index = self.document_index.read();
        if let Some(embedding_ids) = document_index.get(document_id) {
            let embeddings = self.embeddings.read();
            let mut results = Vec::new();

            for embedding_id in embedding_ids {
                if let Some(emb) = embeddings.get(embedding_id) {
                    let dto = EmbeddingDTO {
                        chunk_id: emb.chunk_id.clone(),
                        embedding: emb.embedding.clone(),
                        model_name: emb.model_name.clone(),
                        dimension: emb.embedding.len(), // Derive from vector length
                        computed_at: Utc::now(),
                        model_version: None,
                    };

                    let entity = EmbeddingMapper::from_dto(&dto)?;
                    results.push((entity, emb.embedding.clone()));
                }
            }

            Ok(results)
        } else {
            Ok(Vec::new())
        }
    }

    async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()> {
        let mut chunk_index = self.chunk_index.write();
        if let Some(embedding_id) = chunk_index.remove(chunk_id) {
            self.embeddings.write().remove(&embedding_id);

            // Also remove from document index if tracked
            if let Some(document_id) = self.chunk_to_document.read().get(chunk_id) {
                if let Some(embedding_ids) = self.document_index.write().get_mut(document_id) {
                    embedding_ids.retain(|id| id != &embedding_id);
                }
            }
        }
        Ok(())
    }

    async fn delete_by_document_id(&self, document_id: &str) -> Result<()> {
        let mut document_index = self.document_index.write();
        if let Some(embedding_ids) = document_index.remove(document_id) {
            let mut embeddings = self.embeddings.write();
            let mut chunk_index = self.chunk_index.write();

            for embedding_id in embedding_ids {
                if let Some(emb) = embeddings.remove(&embedding_id) {
                    chunk_index.remove(&emb.chunk_id);
                }
            }
        }
        Ok(())
    }

    async fn count(&self) -> Result<i64> {
        Ok(self.embeddings.read().len() as i64)
    }

    async fn create_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for (entity, vector) in entries {
            let id = entity.id().to_string();
            self.save(&entity, vector).await?;
            ids.push(id);
        }
        Ok(ids)
    }
}
