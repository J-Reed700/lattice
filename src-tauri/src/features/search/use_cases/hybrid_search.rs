//! # Hybrid Search Use Case
//!
//! The library search. Chat's first pass and its follow-up `semantic_search`
//! calls, the tool executor, the search page and the retrieval evaluation's
//! production mode all run this one orchestrator, so a number the evaluation
//! reports describes what chat does.
//!
//! ## Algorithm
//!
//! 1. Embed the query, then run the dense vector branch and the BM25 branch
//!    at once, plus the learned sparse branch when it is switched on and the
//!    loaded model has a sparse head. A failed branch costs the query that
//!    branch, never the others' results.
//! 2. Fuse the ranked lists by weighted reciprocal rank
//!    ([`ReciprocalRankFusion`]), capping how many passages one document may
//!    place before every other document has had a turn.
//! 3. Optionally re-order the head of the fused list with the cross-encoder
//!    ([`HybridSearchUseCase::rerank`]).
//!
//! A caller searching several rewrites of one question collects each query's
//! [`QueryBranches`] and fuses them all in one [`HybridSearchUseCase::fuse`],
//! the same fusion a single query gets.
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::features::search::dto::{SearchModeDto, SearchRequestDto};
//! use lattice::features::search::use_cases::{HybridSearchUseCase, RerankOptions};
//!
//! # async fn example(use_case: HybridSearchUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = SearchRequestDto {
//!     query: "neural network optimization".to_string(),
//!     limit: Some(RerankOptions::candidate_pool(10)),
//!     threshold: None,
//!     mode: SearchModeDto::Hybrid {
//!         vector_weight: 0.7,
//!         bm25_weight: 0.3,
//!     },
//! };
//!
//! let fused = use_case.execute(request).await?;
//! let options = RerankOptions {
//!     query: "neural network optimization".to_string(),
//!     max_candidates: 48,
//!     query_max_chars: 6000,
//! };
//! let mut reranked = use_case.rerank(fused, &options).await.response;
//! reranked.results.truncate(10);
//! # Ok(())
//! # }
//! ```

mod rerank;
#[cfg(test)]
mod tests;

pub use rerank::{RerankOptions, Reranked};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::application::ports::{
    ChunkRepositoryPort, EmbeddingPort, TextSearchPort, VectorSearchPort,
};
use crate::features::search::dto::{
    SearchModeDto, SearchRequestDto, SearchResponseDto, SearchResultDto, SearchResultPortDto,
};
use crate::features::search::engine::fusion::{ReciprocalRankFusion, WeightedRanking};
use crate::features::search::engine::reranker::Reranker;
use crate::features::search::mapper::SearchMapper;
use crate::features::search::SparseSearchTrait;
use crate::shared::error::{AppError, Result};
use crate::shared::text::safe_truncate;
use tracing::{debug, info, warn};

/// Candidates each branch is asked for, per fused result wanted. Fusion can
/// only promote what some branch returned.
const BRANCH_DEPTH: usize = 3;

/// The vector branch's cosine floor in a hybrid search. Low on purpose:
/// fusion orders by rank and never thresholds, so the floor only keeps out
/// neighbours too remote to deserve a rank.
const HYBRID_VECTOR_FLOOR: f32 = 0.15;

/// Passages one document may place in a fused list before every other
/// document has had its turn. The overflow follows, in fused order, so a long
/// document's run of near-identical chunks cannot fill the list alone.
const PER_DOCUMENT_CAP: usize = 4;

/// Where a ranked list came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    /// Dense vector similarity.
    Vector,
    /// BM25 over the chunk text.
    Lexical,
    /// Learned sparse term weights.
    Sparse,
    /// An earlier fused list, unioned with another: chat's corrective retry
    /// fuses its pass with the first. Its results keep the branch
    /// diagnostics they already carry.
    Fused,
}

/// One ranked list and the weight its ranks carry in the fusion.
#[derive(Debug, Clone)]
pub struct RankedBranch {
    pub kind: BranchKind,
    pub weight: f32,
    pub results: Vec<SearchResultDto>,
}

/// Every branch that answered one query, not yet fused.
#[derive(Debug, Default)]
pub struct QueryBranches {
    pub branches: Vec<RankedBranch>,
    /// How many of the vector and BM25 branches answered, so a caller can
    /// tell "nothing matched" (an empty answer) from "search is down" (none).
    pub answered: usize,
}

/// Hybrid search use case: the one library search orchestrator.
///
/// ## Dependencies
///
/// - `EmbeddingPort`: embeds the query
/// - `VectorSearchPort`: the dense branch
/// - `TextSearchPort`: the BM25 branch
/// - optionally a `SparseSearchTrait` branch, a `ChunkRepositoryPort` that
///   supplies chunk bodies and attachment ids, and a `Reranker`
pub struct HybridSearchUseCase {
    embedding_service: Arc<dyn EmbeddingPort>,
    vector_search: Arc<dyn VectorSearchPort>,
    text_search: Arc<dyn TextSearchPort>,
    /// Learned sparse retrieval, when the composition root wired one. Gated
    /// twice: the product switch *and* the loaded model having a sparse head.
    sparse_search: Option<Arc<dyn SparseSearchTrait>>,
    sparse_enabled: bool,
    chunk_repository: Option<Arc<dyn ChunkRepositoryPort>>,
    reranker: Option<Arc<dyn Reranker>>,
    rrf_k: f32,
}

impl HybridSearchUseCase {
    /// Create a new hybrid search use case.
    ///
    /// # Arguments
    ///
    /// * `embedding_service` - Service for generating text embeddings
    /// * `vector_search` - Service for vector similarity search
    /// * `text_search` - Service for BM25 keyword search
    pub fn new(
        embedding_service: Arc<dyn EmbeddingPort>,
        vector_search: Arc<dyn VectorSearchPort>,
        text_search: Arc<dyn TextSearchPort>,
    ) -> Self {
        Self {
            embedding_service,
            vector_search,
            text_search,
            sparse_search: None,
            sparse_enabled: false,
            chunk_repository: None,
            reranker: None,
            rrf_k: crate::shared::constants::DEFAULT_RRF_K,
        }
    }

    /// Read chunk bodies from the repository rather than the index, and leave
    /// chat attachments out of vault-wide vector searches.
    #[must_use]
    pub fn with_chunk_repository(mut self, repository: Arc<dyn ChunkRepositoryPort>) -> Self {
        self.chunk_repository = Some(repository);
        self
    }

    /// Attach the learned sparse branch.
    ///
    /// Wiring it is not the same as turning it on: `enabled` is the product
    /// switch and [`SparseSearchTrait::is_available`] is the capability check,
    /// so a user switching from BGE-M3 to a dense-only model simply goes back
    /// to two-way fusion mid-session.
    #[must_use]
    pub fn with_sparse_search(
        mut self,
        sparse_search: Arc<dyn SparseSearchTrait>,
        enabled: bool,
    ) -> Self {
        self.sparse_search = Some(sparse_search);
        self.sparse_enabled = enabled;
        self
    }

    /// Attach the cross-encoder [`Self::rerank`] uses.
    #[must_use]
    pub fn with_reranker(mut self, reranker: Arc<dyn Reranker>) -> Self {
        self.reranker = Some(reranker);
        self
    }

    /// Override the reciprocal-rank-fusion constant. The application keeps
    /// [`DEFAULT_RRF_K`](crate::shared::constants::DEFAULT_RRF_K); the
    /// evaluation sweeps it.
    #[must_use]
    pub fn with_rrf_k(mut self, rrf_k: f32) -> Self {
        self.rrf_k = rrf_k;
        self
    }

    /// The embedding model the vector branch queries with.
    pub fn model_identity(&self) -> String {
        self.embedding_service.model_identity()
    }

    /// The sparse branch to run for this query, or `None` to keep two-way fusion.
    fn active_sparse_branch(&self) -> Option<&Arc<dyn SparseSearchTrait>> {
        self.sparse_search
            .as_ref()
            .filter(|_| self.sparse_enabled)
            .filter(|sparse| sparse.is_available())
    }

    /// Run the vector branch without blocking the executor.
    ///
    /// The USearch query is synchronous, CPU-bound and can take tens of
    /// milliseconds over a large index. Awaiting it inline inside a `join!`
    /// stalls the whole runtime thread — including the lexical branch that was
    /// supposed to be running beside it.
    ///
    /// A search with no allow-list is a vault-wide one, and a file attached to
    /// a chat is not part of the vault. The index knows nothing of owners, so
    /// the branch asks for that many more neighbours and removes them itself,
    /// before the cut: dropped afterwards, an attachment would cost the caller
    /// a result. A scoped search keeps them, because chat's allow-list names
    /// its own attachments on purpose.
    async fn vector_branch(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        let excluded: HashSet<String> = match (&allowed_document_ids, &self.chunk_repository) {
            (None, Some(repository)) => repository
                .find_conversation_attached_chunk_ids()
                .await?
                .into_iter()
                .collect(),
            _ => HashSet::new(),
        };
        let vector_search = Arc::clone(&self.vector_search);
        let fetch = top_k.saturating_add(excluded.len());
        let mut results = tokio::task::spawn_blocking(move || {
            vector_search.search_scoped(
                &query_embedding,
                fetch,
                threshold,
                allowed_document_ids.as_ref(),
            )
        })
        .await
        .map_err(|error| {
            AppError::InternalError(format!("Vector search task failed: {error}"))
        })??;
        if !excluded.is_empty() {
            results.retain(|result| !excluded.contains(&result.chunk_id));
            results.truncate(top_k);
        }
        if let Some(repository) = &self.chunk_repository {
            let ids = results
                .iter()
                .map(|result| result.chunk_id.clone())
                .collect::<Vec<_>>();
            let content = repository.find_content_by_ids(&ids).await?;
            for result in &mut results {
                if let Some(text) = content.get(&result.chunk_id) {
                    result.content.clone_from(text);
                }
            }
        }
        Ok(results)
    }

    /// Execute a vault-wide search.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - A single-branch search's branch fails
    /// - Every branch of a hybrid search fails
    pub async fn execute(&self, request: SearchRequestDto) -> Result<SearchResponseDto> {
        self.execute_scoped(request, None, None).await
    }

    /// Execute a search under an optional hard scope: a space's memberships
    /// and/or an explicit document allow-list, both applied before the limit.
    ///
    /// `Vector` and `BM25` run that one branch. `Hybrid` runs every branch
    /// ([`Self::search_branches`]) and fuses them ([`Self::fuse`]); it ignores
    /// `threshold`, because its vector floor is part of the fusion's tuning.
    pub async fn execute_scoped(
        &self,
        request: SearchRequestDto,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<SearchResponseDto> {
        let start = std::time::Instant::now();
        let limit = request.limit.unwrap_or(10);
        info!(
            query_len = request.query.len(),
            limit = limit,
            mode = ?request.mode,
            threshold = request.threshold,
            space_scoped = space_id.is_some() || allowed_document_ids.is_some(),
            "HybridSearchUseCase execute"
        );

        // Every arm reports its time when it returns, not here before any
        // branch has run.
        let elapsed_ms = || start.elapsed().as_millis() as u64;
        match request.mode {
            SearchModeDto::Vector => {
                let threshold = request.threshold.unwrap_or(0.5);
                let query_embedding = self.embedding_service.embed_query(&request.query).await?;
                let vector_port_dtos = self
                    .vector_branch(
                        query_embedding,
                        limit,
                        threshold,
                        allowed_document_ids.cloned(),
                    )
                    .await?;
                let vector_results = SearchMapper::port_dtos_to_domain(vector_port_dtos);
                Ok(SearchMapper::to_response_dto(vector_results, elapsed_ms()))
            }
            SearchModeDto::BM25 => {
                let text_port_dtos = self
                    .text_search
                    .search_scoped(&request.query, limit, space_id, allowed_document_ids)
                    .await?;
                let text_results = SearchMapper::port_dtos_to_domain(text_port_dtos);
                Ok(SearchMapper::to_response_dto(text_results, elapsed_ms()))
            }
            SearchModeDto::Hybrid {
                vector_weight,
                bm25_weight,
            } => {
                let query = self
                    .search_branches(
                        &request.query,
                        space_id,
                        allowed_document_ids,
                        limit,
                        vector_weight,
                        bm25_weight,
                    )
                    .await;
                if query.answered == 0 {
                    return Err(AppError::ServiceNotAvailable(
                        "Document search failed in both vector and keyword retrieval".into(),
                    ));
                }
                let results = self.fuse(query.branches, limit);
                debug!(
                    fused_count = results.len(),
                    preview = summarize(&results, 8),
                    "Hybrid fused results"
                );
                Ok(SearchResponseDto {
                    total: results.len(),
                    results,
                    query_time_ms: elapsed_ms(),
                })
            }
        }
    }

    /// One query through every branch, each ranking returned with its fusion
    /// weight and none of them fused yet. Each branch is asked for
    /// `BRANCH_DEPTH` times `limit` candidates.
    ///
    /// The two weights are scaled to sum to one, and two non-positive weights
    /// fuse evenly. When the sparse branch runs and finds anything, it and
    /// BM25 split the lexical weight: both are lexical signals over the same
    /// text, and enabling a branch must not move the vector/lexical balance.
    /// A sparse failure costs the query only that branch.
    pub async fn search_branches(
        &self,
        query: &str,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
        limit: usize,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> QueryBranches {
        let depth = limit.saturating_mul(BRANCH_DEPTH);
        let (vector_weight, lexical_weight) = normalized_weights(vector_weight, bm25_weight);
        let sparse_branch = self.active_sparse_branch();
        let (vector, lexical, sparse) = tokio::join!(
            async {
                let embedding = self.embedding_service.embed_query(query).await?;
                self.vector_branch(
                    embedding,
                    depth,
                    HYBRID_VECTOR_FLOOR,
                    allowed_document_ids.cloned(),
                )
                .await
            },
            self.text_search
                .search_scoped(query, depth, space_id, allowed_document_ids),
            async {
                match sparse_branch {
                    Some(sparse) => {
                        sparse
                            .search_scoped(query, depth, space_id, allowed_document_ids)
                            .await
                    }
                    None => Ok(Vec::new()),
                }
            }
        );
        let sparse = sparse.unwrap_or_else(|error| {
            warn!(%error, "Sparse retrieval branch failed; fusing the other two");
            Vec::new()
        });
        let (bm25_weight, sparse_weight) = split_lexical_weight(lexical_weight, !sparse.is_empty());

        let mut query_branches = QueryBranches::default();
        for (kind, weight, outcome) in [
            (BranchKind::Vector, vector_weight, vector),
            (BranchKind::Lexical, bm25_weight, lexical),
        ] {
            match outcome {
                Ok(port_dtos) => {
                    info!(
                        branch = ?kind,
                        result_count = port_dtos.len(),
                        "Search branch completed"
                    );
                    query_branches.answered += 1;
                    query_branches.branches.push(RankedBranch {
                        kind,
                        weight,
                        results: result_dtos(port_dtos),
                    });
                }
                Err(error) => warn!(branch = ?kind, %error, "A search branch failed"),
            }
        }
        if !sparse.is_empty() {
            query_branches.branches.push(RankedBranch {
                kind: BranchKind::Sparse,
                weight: sparse_weight,
                results: result_dtos(sparse),
            });
        }
        query_branches
    }

    /// Fuse ranked lists into one of at most `limit` results.
    ///
    /// Weighted reciprocal rank decides the order; branch scores are on
    /// incomparable scales and are never mixed. The first list to return a
    /// passage supplies it, with the fused score. No document places more
    /// than `PER_DOCUMENT_CAP` passages ahead of the documents behind it.
    /// Each result reports its best rank, and the score at that rank, in the
    /// vector and BM25 lists it appeared in.
    pub fn fuse(&self, branches: Vec<RankedBranch>, limit: usize) -> Vec<SearchResultDto> {
        let mut payloads: HashMap<String, SearchResultDto> = HashMap::new();
        let mut signals: HashMap<String, BranchSignals> = HashMap::new();
        let mut rankings = Vec::with_capacity(branches.len());
        for branch in branches {
            let mut ids = Vec::with_capacity(branch.results.len());
            let mut seen = HashSet::new();
            for (rank, result) in branch.results.into_iter().enumerate() {
                ids.push(result.id.clone());
                if !seen.insert(result.id.clone()) {
                    continue;
                }
                signals.entry(result.id.clone()).or_default().record(
                    branch.kind,
                    rank,
                    result.score,
                );
                payloads.entry(result.id.clone()).or_insert(result);
            }
            rankings.push(WeightedRanking::new(branch.weight, ids));
        }

        let mut per_document: HashMap<String, usize> = HashMap::new();
        let mut selected = Vec::new();
        let mut deferred = Vec::new();
        for entry in ReciprocalRankFusion::new(self.rrf_k).fuse_ranked(rankings) {
            let Some(mut result) = payloads.remove(&entry.id) else {
                continue;
            };
            result.score = entry.score;
            if let Some(signal) = signals.get(&entry.id) {
                signal.apply(&mut result);
            }
            let placed = per_document
                .entry(result.document_id.clone().unwrap_or_default())
                .or_default();
            if *placed < PER_DOCUMENT_CAP {
                *placed += 1;
                selected.push(result);
            } else {
                deferred.push(result);
            }
        }
        selected.extend(deferred);
        selected.truncate(limit);
        selected
    }
}

/// A result's best rank, and the score at that rank, in the vector and BM25
/// lists of one fusion.
#[derive(Debug, Default)]
struct BranchSignals {
    vector: Option<(usize, f32)>,
    bm25: Option<(usize, f32)>,
}

impl BranchSignals {
    fn record(&mut self, kind: BranchKind, rank: usize, score: f32) {
        let slot = match kind {
            BranchKind::Vector => &mut self.vector,
            BranchKind::Lexical => &mut self.bm25,
            BranchKind::Sparse | BranchKind::Fused => return,
        };
        if slot.is_none_or(|(best, _)| rank < best) {
            *slot = Some((rank, score));
        }
    }

    fn apply(&self, result: &mut SearchResultDto) {
        if let Some((rank, score)) = self.vector {
            result.vector_rank = Some(rank);
            result.vector_score = Some(score);
        }
        if let Some((rank, score)) = self.bm25 {
            result.bm25_rank = Some(rank);
            result.bm25_score = Some(score);
        }
    }
}

/// Branch weights scaled to sum to one. Two non-positive weights mean "no
/// preference", not "no fusion", and fuse evenly.
fn normalized_weights(vector_weight: f32, bm25_weight: f32) -> (f32, f32) {
    let (vector, lexical) = (vector_weight.max(0.0), bm25_weight.max(0.0));
    let total = vector + lexical;
    if total <= f32::EPSILON {
        (0.5, 0.5)
    } else {
        (vector / total, lexical / total)
    }
}

/// The lexical weight as `(bm25, sparse)`: halved between the two when the
/// sparse branch found anything, all BM25's otherwise.
fn split_lexical_weight(lexical_weight: f32, sparse_found: bool) -> (f32, f32) {
    if sparse_found {
        (lexical_weight / 2.0, lexical_weight / 2.0)
    } else {
        (lexical_weight, 0.0)
    }
}

fn result_dtos(port_dtos: Vec<SearchResultPortDto>) -> Vec<SearchResultDto> {
    SearchMapper::port_dtos_to_domain(port_dtos)
        .into_iter()
        .map(SearchMapper::to_dto)
        .collect()
}

fn summarize(results: &[SearchResultDto], max_items: usize) -> String {
    if results.is_empty() {
        return "<none>".to_string();
    }
    results
        .iter()
        .take(max_items)
        .map(|result| {
            format!(
                "{}|score={:.4}|doc={}|snippet=\"{}\"",
                result.id,
                result.score,
                result.document_id.as_deref().unwrap_or("-"),
                safe_truncate(&result.content, 48)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}
