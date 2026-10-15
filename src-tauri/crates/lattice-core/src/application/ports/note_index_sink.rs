//! Corpus sink for mirrored vault notes.
//!
//! The vault writer is started before the DI container exists, so it cannot
//! hold one. The composition root installs a sink once the container is
//! built; the writer calls it to put a mirrored note into (or out of) the
//! index. A missing sink — boot order, test fixtures — is reported as an
//! error so the durable outbox entry stays deferred for the next pass,
//! exactly like a missing Tauri handle used to be.

use async_trait::async_trait;

use crate::shared::error::Result;

#[async_trait]
pub trait NoteIndexSink: Send + Sync {
    /// Submit the file to the indexing pipeline.
    async fn index(&self, path: &str) -> Result<()>;

    /// Drop the file from the index. Idempotent: a path that was never
    /// indexed is already gone.
    async fn remove(&self, path: &str) -> Result<()>;
}
