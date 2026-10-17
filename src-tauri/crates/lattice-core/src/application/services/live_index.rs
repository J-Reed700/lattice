//! Publishing committed chunks into the live vector index.

use std::sync::Arc;

use crate::application::ports::vector_search_port::VectorIndexEntry;
use crate::application::ports::{SearchResultCachePort, VectorSearchPort};
use crate::shared::error::{AppError, Result};

/// One committed chunk ready for the live vector index.
pub struct PublishedChunk {
    pub chunk_id: String,
    pub content: String,
    pub embedding: Vec<f32>,
}

/// Put a committed document's chunks into the live vector index and drop
/// cached query results, so the document is searchable now rather than after
/// the next launch rebuilds the index from SQLite.
///
/// Shared by every importer that commits chunk rows itself (files, web URLs):
/// a path that commits without publishing leaves dense search blind to the
/// document and forces a full HNSW rebuild at the next start.
pub async fn publish_chunks_to_live_index(
    search: Arc<dyn VectorSearchPort>,
    query_cache: Option<&dyn SearchResultCachePort>,
    document_id: &str,
    chunks: Vec<PublishedChunk>,
) -> Result<()> {
    let entries: Vec<_> = chunks
        .into_iter()
        .map(|chunk| VectorIndexEntry {
            id: format!("emb_{}", chunk.chunk_id),
            embedding: chunk.embedding,
            content: chunk.content,
            chunk_id: chunk.chunk_id,
            document_id: document_id.to_owned(),
        })
        .collect();
    // HNSW updates and disk serialization must not block async DB/chat work.
    tokio::task::spawn_blocking(move || search.publish_embeddings(entries)).await
        .map_err(|error| AppError::Other(format!("Search indexing task failed: {error}")))?
        .map_err(|error| AppError::Other(format!("Document saved, but search indexing failed: {error}. Retry this file to finish indexing.")))?;
    if let Some(cache) = query_cache {
        cache.invalidate_corpus();
    }
    Ok(())
}
