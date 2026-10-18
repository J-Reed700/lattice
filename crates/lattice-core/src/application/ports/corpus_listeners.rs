//! What the indexing pipeline tells when the corpus changes.
//!
//! Search caches results against the corpus as it was, and the summary tier
//! follows documents into and out of the index. Indexing knows neither
//! feature; the composition root hands each indexer the instances it built,
//! so every indexer in one app tells the same cache and the same tier.

use async_trait::async_trait;

/// Search results cached against the corpus as it was.
pub trait SearchResultCachePort: Send + Sync {
    /// The corpus changed: results cached before now must not be served.
    fn invalidate_corpus(&self);
}

/// Work that follows a document into, and out of, the index. Neither call
/// may fail the indexing operation that makes it, so neither returns an error.
#[async_trait]
pub trait DocumentIndexObserver: Send + Sync {
    /// The document's chunks and vectors are committed and searchable.
    async fn document_indexed(&self, document_id: &str);

    /// The document is about to be deleted; its rows still exist.
    async fn document_deleted(&self, document_id: &str);
}
