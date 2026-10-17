//! The search index a freshly loaded embedding model binds before it is
//! published.
//!
//! Search implements it, so the embedding loader never names the index it
//! fills. The implementation holds its own database handle.

use async_trait::async_trait;

use crate::application::ports::EmbeddingPort;
use crate::shared::error::Result;

/// Binds the first loaded model's vector space to the shared search index.
#[async_trait]
pub trait EmbeddingIndexBinding: Send + Sync {
    /// Bind the vector space of `model`, or fail when the index already holds
    /// a different one.
    async fn bind_first(&self, model: &dyn EmbeddingPort, late_chunking: bool) -> Result<()>;
}
