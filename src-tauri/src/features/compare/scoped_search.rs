//! The library search compare drives, behind a compare-owned port.
//!
//! Compare needs exactly one thing from the library: one hybrid query per
//! column, hard-scoped to the document under comparison, fused. Declaring
//! that as a port here — instead of naming the search feature's use case
//! and DTOs in the pipeline — is what keeps the compare → search edge at a
//! single reference: this file, and only this file, sees search's types.
//! The plugin binds [`HybridScopedSearch`] over the hybrid use case, the
//! way every other compare dependency is bound in one place.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::features::search::use_cases::hybrid_search::HybridSearchUseCase;
use crate::shared::error::Result;

/// One fused hit, reduced to what a compare cell can cite: the identity,
/// the text, and where it sits in the document.
#[derive(Debug, Clone)]
pub struct ScopedHit {
    pub id: String,
    pub content: String,
    pub position: Option<usize>,
}

/// One hybrid query per column, scoped to the given documents: the library
/// search's vector, BM25 and sparse branches, fused, nothing else.
#[async_trait]
pub trait ScopedLibrarySearch: Send + Sync {
    /// Search `column` across `document_ids`, best `limit` hits first.
    async fn search_column(
        &self,
        column: &str,
        document_ids: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<ScopedHit>>;
}

/// The binding: the library's hybrid search, scoped the way compare asks.
///
/// Compare's request is always the app's default hybrid mode at the
/// application's fusion weights — the same request chat and the search
/// page send — so the port takes only the column and the scope.
pub struct HybridScopedSearch(Arc<HybridSearchUseCase>);

impl HybridScopedSearch {
    pub fn new(search: Arc<HybridSearchUseCase>) -> Self {
        Self(search)
    }
}

#[async_trait]
impl ScopedLibrarySearch for HybridScopedSearch {
    async fn search_column(
        &self,
        column: &str,
        document_ids: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<ScopedHit>> {
        let response = self
            .0
            .execute_hybrid_scoped(column, limit, Some(document_ids))
            .await?;
        Ok(response
            .results
            .into_iter()
            .map(|result| ScopedHit {
                id: result.id,
                content: result.content,
                position: result.position,
            })
            .collect())
    }
}
