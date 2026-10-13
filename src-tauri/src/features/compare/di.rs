//! Compare's dependencies, bound as one small named bundle.
//!
//! The pipeline needs four things and nothing else: a model to answer with,
//! the document repository to resolve titles, the library search to find the
//! passages a column asks about, and the chunk repository for the fallback.
//! The command layer assembles this from the container; the pipeline never
//! sees the container itself.

use std::sync::Arc;

use crate::application::ports::llm_port::LLMPort;
use crate::application::ports::{ChunkRepositoryPort, DocumentRepository};
use crate::features::compare::scoped_search::ScopedLibrarySearch;
use crate::shared::error::Result;

type BoxFuture<T> = futures::future::BoxFuture<'static, T>;

/// One model load: the future a [`LlmLoader`] hands back when asked.
type LlmLoad = BoxFuture<Result<Arc<dyn LLMPort>>>;

/// Lazily resolves the chat model without exposing the container.
///
/// Mirrors [`crate::application::ports::llm_port::OptionalLlmLoader`] but
/// compare needs a model to answer with, so this one is non-optional: a
/// missing model is the pipeline's one whole-request failure. One loader
/// shares one container, so repeated loads hit the model cache.
#[derive(Clone)]
pub struct LlmLoader(Arc<dyn Fn() -> LlmLoad + Send + Sync>);

impl LlmLoader {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn() -> LlmLoad + Send + Sync + 'static,
    {
        Self(Arc::new(f))
    }

    pub fn load(&self) -> LlmLoad {
        (self.0)()
    }
}

/// Everything the compare pipeline needs, in one place.
#[derive(Clone)]
pub struct CompareDeps {
    /// Loads the chat model on demand.
    pub load_llm: LlmLoader,
    /// Resolves document ids to titles and paths.
    pub documents: Arc<dyn DocumentRepository>,
    /// Scoped library search, one branch per column.
    pub search: Arc<dyn ScopedLibrarySearch>,
    /// Opening-chunk fallback when a search finds nothing.
    pub chunks: Arc<dyn ChunkRepositoryPort>,
}
