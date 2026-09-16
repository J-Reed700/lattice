//! Persistence port for learned sparse term weights.
//!
//! Dense vectors have two homes already (`text_embeddings` and the USearch
//! index). Learned sparse weights need a third: a term-id → weight posting per
//! chunk, keyed by the model identity that produced it. This port is what the
//! indexing use case writes through, so the indexer keeps depending only on
//! ports and the SQL lives in `features::search::engine::sparse_search`.

use async_trait::async_trait;

use crate::domain::value_objects::SparseEmbedding;
use crate::shared::result::Result;

/// One chunk's sparse posting, ready to persist.
pub type ChunkSparseTerms = (String, SparseEmbedding);

/// Stores per-chunk learned sparse term weights.
#[async_trait]
pub trait SparseTermStorePort: Send + Sync {
    /// Replace the sparse postings of the given chunks under `model_identity`.
    ///
    /// Implementations must be idempotent: re-indexing the same document twice
    /// leaves exactly one posting per (chunk, model, term). Entries with an
    /// empty [`SparseEmbedding`] still clear whatever was stored for that
    /// chunk, so a chunk that stops activating any term does not keep serving
    /// stale terms.
    async fn replace_chunk_terms(
        &self,
        model_identity: &str,
        entries: &[ChunkSparseTerms],
    ) -> Result<()>;

    /// Drop every posting for these chunks, in every model identity.
    ///
    /// The schema cascades from `text_chunks`, so this exists for callers that
    /// remove vectors without removing the chunk rows themselves.
    async fn delete_chunk_terms(&self, chunk_ids: &[String]) -> Result<()>;
}
