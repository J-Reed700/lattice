//! First-stage, collection-level retrieval over summary vectors.
//!
//! Summaries live in their own USearch file (`summaries-<identity>.usearch`),
//! never in the chunk index, so a summary can never be returned as a passage
//! and quoted as evidence. What it can do is name the *documents* worth
//! reading, which is what "what is this about" and "where do I start" need.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::application::ports::vector_search_port::VectorSearchPort;
use crate::application::ports::EmbeddingPort;
use crate::features::summaries::entity::SummaryHit;
use crate::features::summaries::repository::SummaryRepositoryPort;
use crate::shared::error::Result;

/// Over-fetch factor: scoped search filters after retrieval, and one document
/// can own several section summaries, so the window has to be wider than the
/// number of documents we mean to return.
const CANDIDATE_FACTOR: usize = 6;

/// Port for the summary tier, so callers can be tested against a stub.
#[async_trait]
pub trait SummarySearchPort: Send + Sync {
    /// Best summaries for `query`, restricted to `scope`, best first.
    async fn top_summaries(
        &self,
        query: &str,
        scope: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<SummaryHit>>;
}

pub struct SummarySearch {
    index: Arc<dyn VectorSearchPort>,
    repository: Arc<dyn SummaryRepositoryPort>,
    embedder: Arc<dyn EmbeddingPort>,
}

impl SummarySearch {
    pub fn new(
        index: Arc<dyn VectorSearchPort>,
        repository: Arc<dyn SummaryRepositoryPort>,
        embedder: Arc<dyn EmbeddingPort>,
    ) -> Self {
        Self {
            index,
            repository,
            embedder,
        }
    }
}

impl SummarySearch {
    /// Document-level summary text for `scope`, keyed by document id.
    ///
    /// Only whole-document summaries: a planner catalog describes documents,
    /// and splicing a section summary in under a document's name would be
    /// telling the planner a chapter is the book.
    pub async fn document_summaries(
        &self,
        scope: &HashSet<String>,
    ) -> Result<std::collections::HashMap<String, String>> {
        let summaries = self
            .repository
            .list_for_documents(scope, &self.embedder.model_identity())
            .await?;
        Ok(crate::features::summaries::repository::document_level_text(
            &summaries,
        ))
    }
}

#[async_trait]
impl SummarySearchPort for SummarySearch {
    async fn top_summaries(
        &self,
        query: &str,
        scope: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<SummaryHit>> {
        if limit == 0 || scope.is_empty() || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let embedding = self.embedder.embed_query(query).await?;
        let candidates = self.index.search_scoped(
            &embedding,
            limit.saturating_mul(CANDIDATE_FACTOR).max(limit),
            0.0,
            Some(scope),
        )?;
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        // SQLite is authoritative. A vector whose row is gone belongs to a
        // deleted document or a rotated identity and is not a hit.
        let ids: Vec<String> = candidates
            .iter()
            .map(|candidate| candidate.chunk_id.clone())
            .collect();
        let rows = self.repository.find_by_ids(&ids).await?;
        let mut hits: Vec<SummaryHit> = candidates
            .iter()
            .filter_map(|candidate| {
                let row = rows.iter().find(|row| row.id == candidate.chunk_id)?;
                Some(SummaryHit {
                    document_id: row.document_id.clone(),
                    summary_id: row.id.clone(),
                    level: row.level,
                    section: row.section.clone(),
                    summary_text: row.summary_text.clone(),
                    score: candidate.score,
                })
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.summary_id.cmp(&b.summary_id))
        });
        hits.truncate(limit);
        Ok(hits)
    }
}
