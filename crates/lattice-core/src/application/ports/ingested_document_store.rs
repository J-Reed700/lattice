//! Storage for a document an importer chunked and embedded itself, such as a
//! web article.

use std::path::Path;

use async_trait::async_trait;

use crate::domain::value_objects::ContextualizedChunk;
use crate::shared::error::Result;

/// Stores a pre-chunked, pre-embedded document and reports its chunk rows.
#[async_trait]
pub trait IngestedDocumentStorePort: Send + Sync {
    /// Store the document with its contextualized chunks and their vectors,
    /// stamping the vectors with `model_identity`. Returns the document id.
    async fn store_document_with_context_for_model(
        &self,
        path: &Path,
        mime_type: &str,
        chunks: Vec<ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
        model_identity: &str,
    ) -> Result<String>;

    /// The stored chunk ids of `document_id`, in chunk order.
    async fn chunk_ids_in_order(&self, document_id: &str) -> Result<Vec<String>>;
}
