use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use super::{
    normalized_weights, split_lexical_weight, BranchKind, HybridSearchUseCase, RankedBranch,
    RerankOptions,
};
use crate::application::ports::{EmbeddingPort, TextSearchPort, VectorSearchPort};
use crate::features::search::dto::{
    SearchModeDto, SearchRequestDto, SearchResponseDto, SearchResultDto, SearchResultPortDto,
};
use crate::features::search::engine::reranker::{RerankResult, Reranker};
use crate::features::search::SparseSearchTrait;
use crate::shared::error::{AppError, Result};

fn port_hit(chunk: &str, document: &str, score: f32) -> SearchResultPortDto {
    SearchResultPortDto {
        doc_id: document.into(),
        chunk_id: chunk.into(),
        score,
        content: format!("{chunk} text"),
    }
}

fn hit(id: &str, document: &str, score: f32) -> SearchResultDto {
    SearchResultDto {
        id: id.into(),
        title: document.into(),
        content: id.into(),
        score,
        path: None,
        document_id: Some(document.into()),
        position: None,
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata: std::collections::HashMap::new(),
    }
}

fn branch(kind: BranchKind, weight: f32, results: Vec<SearchResultDto>) -> RankedBranch {
    RankedBranch {
        kind,
        weight,
        results,
    }
}

fn ids(results: &[SearchResultDto]) -> Vec<&str> {
    results.iter().map(|result| result.id.as_str()).collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-6
}

/// Every query embeds to the same vector; the scripted index decides the hits.
struct FixedEmbedder;

#[async_trait::async_trait]
impl EmbeddingPort for FixedEmbedder {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![1.0, 0.0])
    }
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
    }
    fn dimension(&self) -> usize {
        2
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A vector index that answers every query with the same ranked hits.
struct ScriptedVectorSearch {
    hits: Vec<SearchResultPortDto>,
}

impl VectorSearchPort for ScriptedVectorSearch {
    fn search(
        &self,
        _embedding: &[f32],
        limit: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResultPortDto>> {
        Ok(self
            .hits
            .iter()
            .filter(|hit| hit.score >= threshold)
            .take(limit)
            .cloned()
            .collect())
    }

    fn search_scoped(
        &self,
        embedding: &[f32],
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        let mut hits = self.search(embedding, usize::MAX, threshold)?;
        if let Some(allowed) = allowed_document_ids {
            hits.retain(|hit| allowed.contains(&hit.doc_id));
        }
        hits.truncate(top_k);
        Ok(hits)
    }

    fn add_embedding(&self, _id: String, _embedding: Vec<f32>) -> Result<()> {
        Ok(())
    }
    fn remove_embedding(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    fn clear(&self) -> Result<()> {
        Ok(())
    }
    fn count(&self) -> usize {
        self.hits.len()
    }
    fn dimension(&self) -> usize {
        2
    }
}

/// A keyword index that answers every query with the same ranked hits, or
/// fails, and remembers how many candidates it was asked for.
struct ScriptedTextSearch {
    hits: Vec<SearchResultPortDto>,
    down: bool,
    asked_for: Mutex<Vec<usize>>,
}

impl ScriptedTextSearch {
    fn answering(hits: Vec<SearchResultPortDto>) -> Self {
        Self {
            hits,
            down: false,
            asked_for: Mutex::new(Vec::new()),
        }
    }

    fn down() -> Self {
        Self {
            hits: Vec::new(),
            down: true,
            asked_for: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl TextSearchPort for ScriptedTextSearch {
    async fn search(&self, _query: &str, top_k: usize) -> Result<Vec<SearchResultPortDto>> {
        self.asked_for.lock().unwrap().push(top_k);
        if self.down {
            return Err(AppError::Database(
                "the keyword index is unavailable".into(),
            ));
        }
        Ok(self.hits.iter().take(top_k).cloned().collect())
    }
    async fn index_document(&self, _id: &str, _content: &str) -> Result<()> {
        Ok(())
    }
    async fn index_batch(&self, _documents: &[(&str, &str)]) -> Result<()> {
        Ok(())
    }
    async fn remove_document(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    async fn clear(&self) -> Result<()> {
        Ok(())
    }
    async fn count(&self) -> Result<usize> {
        Ok(self.hits.len())
    }
}

/// A sparse branch that is always available and always returns `hits`.
struct ScriptedSparseSearch {
    hits: Vec<SearchResultPortDto>,
}

#[async_trait::async_trait]
impl SparseSearchTrait for ScriptedSparseSearch {
    fn is_available(&self) -> bool {
        true
    }
    async fn search_scoped(
        &self,
        _query: &str,
        top_k: usize,
        _space_id: Option<&str>,
        _allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        Ok(self.hits.iter().take(top_k).cloned().collect())
    }
}

fn use_case(
    vector: Vec<SearchResultPortDto>,
    text: Vec<SearchResultPortDto>,
) -> HybridSearchUseCase {
    HybridSearchUseCase::new(
        Arc::new(FixedEmbedder),
        Arc::new(ScriptedVectorSearch { hits: vector }),
        Arc::new(ScriptedTextSearch::answering(text)),
    )
}

fn hybrid(query: &str, limit: usize) -> SearchRequestDto {
    SearchRequestDto {
        query: query.into(),
        limit: Some(limit),
        threshold: None,
        mode: SearchModeDto::Hybrid {
            vector_weight: 0.7,
            bm25_weight: 0.3,
        },
    }
}

/// A keyword branch that takes a measurable while.
struct SlowTextSearch;

#[async_trait::async_trait]
impl TextSearchPort for SlowTextSearch {
    async fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<SearchResultPortDto>> {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        Ok(Vec::new())
    }
    async fn index_document(&self, _id: &str, _content: &str) -> Result<()> {
        Ok(())
    }
    async fn index_batch(&self, _documents: &[(&str, &str)]) -> Result<()> {
        Ok(())
    }
    async fn remove_document(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    async fn clear(&self) -> Result<()> {
        Ok(())
    }
    async fn count(&self) -> Result<usize> {
        Ok(0)
    }
}

#[tokio::test]
async fn query_time_covers_the_search_itself() {
    let use_case = HybridSearchUseCase::new(
        Arc::new(crate::application::ports::mock_embedding_port::MockEmbeddingPort::new_degraded()),
        Arc::new(ScriptedVectorSearch { hits: Vec::new() }),
        Arc::new(SlowTextSearch),
    );

    let response = use_case
        .execute(SearchRequestDto {
            query: "anything".into(),
            limit: Some(5),
            threshold: None,
            mode: SearchModeDto::BM25,
        })
        .await
        .unwrap();

    assert!(response.query_time_ms >= 30, "{}", response.query_time_ms);
}

// ---------------------------------------------------------------------------
// Fusion
// ---------------------------------------------------------------------------

#[test]
fn fusion_ranks_across_score_scales_and_caps_each_document() {
    let search = use_case(Vec::new(), Vec::new());
    let vector = vec![hit("v", "a", 0.99), hit("shared", "b", 0.4)];
    let lexical = vec![hit("shared", "b", 100.0), hit("k", "c", 90.0)];
    let fused = search.fuse(
        vec![
            branch(BranchKind::Vector, 0.7, vector),
            branch(BranchKind::Lexical, 0.3, lexical),
        ],
        3,
    );
    assert_eq!(fused[0].id, "shared");

    let mut crowded: Vec<_> = (0..12).map(|i| hit(&format!("a{i}"), "a", 1.0)).collect();
    crowded.push(hit("b0", "b", 0.1));
    let diverse = search.fuse(vec![branch(BranchKind::Fused, 1.0, crowded)], 8);
    assert_eq!(diverse[4].id, "b0");
    assert_eq!(diverse.len(), 8);
}

#[test]
fn fused_results_report_their_rank_and_score_in_each_branch() {
    let search = use_case(Vec::new(), Vec::new());
    let fused = search.fuse(
        vec![
            branch(
                BranchKind::Vector,
                0.7,
                vec![hit("v", "a", 0.9), hit("shared", "b", 0.4)],
            ),
            branch(BranchKind::Lexical, 0.3, vec![hit("shared", "b", 7.0)]),
        ],
        5,
    );

    let shared = fused.iter().find(|r| r.id == "shared").unwrap();
    assert_eq!(shared.vector_rank, Some(1));
    assert_eq!(shared.vector_score, Some(0.4));
    assert_eq!(shared.bm25_rank, Some(0));
    assert_eq!(shared.bm25_score, Some(7.0));
    let vector_only = fused.iter().find(|r| r.id == "v").unwrap();
    assert_eq!(vector_only.bm25_rank, None);
    assert_eq!(vector_only.bm25_score, None);
}

/// Re-fusing an earlier pass must not erase what that pass learned about its
/// branches.
#[test]
fn a_fused_pass_keeps_its_branch_diagnostics_when_fused_again() {
    let search = use_case(Vec::new(), Vec::new());
    let mut earlier = hit("x", "a", 0.05);
    earlier.vector_rank = Some(3);
    earlier.bm25_rank = Some(0);

    let fused = search.fuse(vec![branch(BranchKind::Fused, 1.0, vec![earlier])], 5);

    assert_eq!(fused[0].vector_rank, Some(3));
    assert_eq!(fused[0].bm25_rank, Some(0));
}

#[test]
fn a_sparse_only_hit_still_reaches_the_fused_list() {
    let search = use_case(Vec::new(), Vec::new());
    let fused = search.fuse(
        vec![
            branch(BranchKind::Vector, 0.7, vec![hit("dense", "a", 0.9)]),
            branch(BranchKind::Lexical, 0.15, vec![hit("lexical", "b", 5.0)]),
            branch(BranchKind::Sparse, 0.15, vec![hit("sparse", "c", 3.0)]),
        ],
        10,
    );
    assert!(ids(&fused).contains(&"sparse"), "{:?}", ids(&fused));
}

/// Two lexical branches agreeing on a passage still do not outvote one
/// confident dense hit: 0.7/11 against 0.15/11 + 0.15/11.
#[test]
fn a_sparse_branch_cannot_outvote_a_confident_vector_hit() {
    let search = use_case(Vec::new(), Vec::new());
    let fused = search.fuse(
        vec![
            branch(BranchKind::Vector, 0.7, vec![hit("dense", "a", 0.9)]),
            branch(BranchKind::Lexical, 0.15, vec![hit("lexical", "b", 5.0)]),
            branch(BranchKind::Sparse, 0.15, vec![hit("lexical", "b", 5.0)]),
        ],
        10,
    );
    assert_eq!(ids(&fused), ["dense", "lexical"]);
}

/// Degenerate weights mean "no preference", not "no fusion".
#[test]
fn degenerate_weights_fuse_with_equal_weights() {
    assert_eq!(normalized_weights(0.0, 0.0), (0.5, 0.5));
    assert_eq!(normalized_weights(-1.0, 0.0), (0.5, 0.5));
    let (vector, lexical) = normalized_weights(0.7, 0.3);
    assert!(close(vector, 0.7) && close(lexical, 0.3));
}

/// Turning the sparse branch on must not move the vector/lexical balance.
#[tokio::test]
async fn a_sparse_branch_takes_half_the_lexical_weight_rather_than_adding_one() {
    let search = use_case(
        vec![port_hit("dense", "a", 0.9)],
        vec![port_hit("lexical", "b", 1.0)],
    )
    .with_sparse_search(
        Arc::new(ScriptedSparseSearch {
            hits: vec![port_hit("lexical", "b", 2.0)],
        }),
        true,
    );

    let query = search
        .search_branches("retention policy", None, None, 5, 0.7, 0.3)
        .await;

    let weight = |kind| {
        query
            .branches
            .iter()
            .find(|branch| branch.kind == kind)
            .map(|branch| branch.weight)
    };
    assert!(close(weight(BranchKind::Vector).unwrap(), 0.7));
    assert!(close(weight(BranchKind::Lexical).unwrap(), 0.15));
    assert!(close(weight(BranchKind::Sparse).unwrap(), 0.15));
    assert_eq!(split_lexical_weight(0.3, false), (0.3, 0.0));
}

#[tokio::test]
async fn a_switched_off_sparse_branch_never_runs() {
    let search = use_case(
        vec![port_hit("dense", "a", 0.9)],
        vec![port_hit("lexical", "b", 1.0)],
    )
    .with_sparse_search(
        Arc::new(ScriptedSparseSearch {
            hits: vec![port_hit("sparse", "c", 2.0)],
        }),
        false,
    );

    let query = search
        .search_branches("retention policy", None, None, 5, 0.7, 0.3)
        .await;

    assert!(query
        .branches
        .iter()
        .all(|branch| branch.kind != BranchKind::Sparse));
}

// ---------------------------------------------------------------------------
// Hybrid search
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_hybrid_search_fuses_both_branches() {
    let search = use_case(
        vec![port_hit("v1", "a", 0.9), port_hit("both", "b", 0.5)],
        vec![port_hit("both", "b", 1.0), port_hit("k1", "c", 0.8)],
    );

    let response = search.execute(hybrid("anything", 5)).await.unwrap();

    assert_eq!(ids(&response.results), ["both", "v1", "k1"]);
    assert_eq!(response.total, 3);
}

/// Each branch is asked for three candidates per fused result: fusion can
/// only promote what some branch returned.
#[tokio::test]
async fn a_hybrid_search_asks_each_branch_for_three_candidates_per_result() {
    let text = Arc::new(ScriptedTextSearch::answering(vec![port_hit("k", "a", 1.0)]));
    let search = HybridSearchUseCase::new(
        Arc::new(FixedEmbedder),
        Arc::new(ScriptedVectorSearch { hits: Vec::new() }),
        text.clone(),
    );

    search.execute(hybrid("anything", 5)).await.unwrap();

    assert_eq!(*text.asked_for.lock().unwrap(), [15]);
}

#[tokio::test]
async fn a_hybrid_search_keeps_the_branch_that_answered() {
    let search = HybridSearchUseCase::new(
        Arc::new(crate::application::ports::mock_embedding_port::MockEmbeddingPort::new_degraded()),
        Arc::new(ScriptedVectorSearch {
            hits: vec![port_hit("v", "a", 0.9)],
        }),
        Arc::new(ScriptedTextSearch::answering(vec![port_hit("k", "b", 1.0)])),
    );

    let response = search.execute(hybrid("anything", 5)).await.unwrap();

    assert_eq!(ids(&response.results), ["k"]);
}

#[tokio::test]
async fn a_hybrid_search_with_every_branch_down_is_an_error() {
    let search = HybridSearchUseCase::new(
        Arc::new(crate::application::ports::mock_embedding_port::MockEmbeddingPort::new_degraded()),
        Arc::new(ScriptedVectorSearch { hits: Vec::new() }),
        Arc::new(ScriptedTextSearch::down()),
    );

    assert!(search.execute(hybrid("anything", 5)).await.is_err());
}

#[tokio::test]
async fn a_hybrid_search_stays_inside_its_allow_list() {
    let search = use_case(
        vec![port_hit("inside", "a", 0.9), port_hit("outside", "z", 0.95)],
        vec![port_hit("outside", "z", 1.0)],
    );
    let allowed = HashSet::from(["a".to_owned()]);

    let response = search
        .execute_scoped(hybrid("anything", 5), None, Some(&allowed))
        .await
        .unwrap();

    assert_eq!(ids(&response.results), ["inside"]);
}

// ---------------------------------------------------------------------------
// Vault-wide searches and chat attachments
// ---------------------------------------------------------------------------

async fn attachment_fixture() -> Arc<dyn crate::application::ports::ChunkRepositoryPort> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE documents (id TEXT PRIMARY KEY, owner_conversation_id TEXT);
         CREATE TABLE text_chunks (id TEXT PRIMARY KEY, document_id TEXT, content TEXT);
         INSERT INTO documents VALUES ('library', NULL), ('attached', 'some-chat');
         INSERT INTO text_chunks VALUES ('library-chunk', 'library', 'library text'),
                                        ('attached-chunk', 'attached', 'attached text');",
    )
    .execute(&pool)
    .await
    .unwrap();
    Arc::new(
        crate::infrastructure::persistence::repositories::chunk_repository::ChunkRepository::new(
            pool,
        ),
    )
}

fn attachment_search(
    chunks: Arc<dyn crate::application::ports::ChunkRepositoryPort>,
) -> HybridSearchUseCase {
    // The attachment is the nearer neighbour.
    use_case(
        vec![
            port_hit("attached-chunk", "attached", 0.95),
            port_hit("library-chunk", "library", 0.9),
        ],
        Vec::new(),
    )
    .with_chunk_repository(chunks)
}

fn vector(limit: usize) -> SearchRequestDto {
    SearchRequestDto {
        query: "anything".into(),
        limit: Some(limit),
        threshold: Some(0.5),
        mode: SearchModeDto::Vector,
    }
}

#[tokio::test]
async fn a_chat_attachment_does_not_take_a_vault_wide_vector_result_slot() {
    let search = attachment_search(attachment_fixture().await);

    let response = search.execute(vector(1)).await.unwrap();

    assert_eq!(ids(&response.results), ["library-chunk"]);
}

/// Chat's allow-list names its own attachments on purpose.
#[tokio::test]
async fn a_scoped_search_keeps_the_attachments_its_allow_list_names() {
    let search = attachment_search(attachment_fixture().await);
    let allowed = HashSet::from(["attached".to_owned(), "library".to_owned()]);

    let response = search
        .execute_scoped(vector(1), None, Some(&allowed))
        .await
        .unwrap();

    assert_eq!(ids(&response.results), ["attached-chunk"]);
}

// ---------------------------------------------------------------------------
// Rerank stage
// ---------------------------------------------------------------------------

/// A cross-encoder that scores the documents it is given with `scores`, in
/// order, or fails when there are none; it records every call.
#[derive(Debug)]
struct ScriptedReranker {
    available: bool,
    scores: Option<Vec<f32>>,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

impl ScriptedReranker {
    fn scoring(scores: Vec<f32>) -> Arc<Self> {
        Arc::new(Self {
            available: true,
            scores: Some(scores),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn failing() -> Arc<Self> {
        Arc::new(Self {
            available: true,
            scores: None,
            calls: Mutex::new(Vec::new()),
        })
    }

    fn unavailable() -> Arc<Self> {
        Arc::new(Self {
            available: false,
            scores: Some(vec![0.0, 0.0, 1.0]),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Reranker for ScriptedReranker {
    fn is_available(&self) -> bool {
        self.available
    }

    async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        _top_k: usize,
    ) -> Result<Vec<RerankResult>> {
        self.calls
            .lock()
            .unwrap()
            .push((query.to_owned(), documents.clone()));
        let Some(scores) = &self.scores else {
            return Err(AppError::InternalError("the cross-encoder crashed".into()));
        };
        Ok(scores
            .iter()
            .take(documents.len())
            .enumerate()
            .map(|(index, score)| RerankResult {
                index,
                score: *score,
            })
            .collect())
    }
}

fn fused_list() -> SearchResponseDto {
    let results = vec![
        hit("first", "a", 0.09),
        hit("second", "b", 0.08),
        hit("third", "c", 0.07),
    ];
    SearchResponseDto {
        total: results.len(),
        results,
        query_time_ms: 0,
    }
}

fn rerank_options(max_candidates: usize) -> RerankOptions {
    RerankOptions {
        query: "which passage answers".into(),
        max_candidates,
        query_max_chars: 6000,
    }
}

fn reranking_with(reranker: Arc<ScriptedReranker>) -> HybridSearchUseCase {
    use_case(Vec::new(), Vec::new()).with_reranker(reranker)
}

#[tokio::test]
async fn reranking_reorders_the_list_by_blended_score_and_says_so() {
    let reranker = ScriptedReranker::scoring(vec![0.0, 0.0, 1.0]);
    let search = reranking_with(reranker.clone());

    let reranked = search.rerank(fused_list(), &rerank_options(48)).await;

    assert!(reranked.applied);
    assert_eq!(
        ids(&reranked.response.results),
        ["third", "first", "second"]
    );
    let calls = reranker.calls();
    assert_eq!(calls.len(), 1);
    // Each passage is judged with its title in front of it.
    assert_eq!(calls[0].1[0], "a first");
}

#[tokio::test]
async fn an_unavailable_reranker_keeps_the_fused_order() {
    let reranker = ScriptedReranker::unavailable();
    let search = reranking_with(reranker.clone());

    let reranked = search.rerank(fused_list(), &rerank_options(48)).await;

    assert!(!reranked.applied);
    assert_eq!(
        ids(&reranked.response.results),
        ["first", "second", "third"]
    );
    assert!(reranker.calls().is_empty());
}

#[tokio::test]
async fn a_search_with_no_reranker_keeps_the_fused_order() {
    let search = use_case(Vec::new(), Vec::new());

    let reranked = search.rerank(fused_list(), &rerank_options(48)).await;

    assert!(!reranked.applied);
    assert_eq!(
        ids(&reranked.response.results),
        ["first", "second", "third"]
    );
}

#[tokio::test]
async fn a_failed_rerank_keeps_the_fused_order_and_scores() {
    let search = reranking_with(ScriptedReranker::failing());

    let reranked = search.rerank(fused_list(), &rerank_options(48)).await;

    assert!(!reranked.applied);
    assert_eq!(
        ids(&reranked.response.results),
        ["first", "second", "third"]
    );
    assert_eq!(reranked.response.results[0].score, 0.09);
}

/// One score for three candidates cannot be blended; nothing is half-applied.
#[tokio::test]
async fn a_malformed_rerank_keeps_the_fused_order() {
    let search = reranking_with(ScriptedReranker::scoring(vec![1.0]));

    let reranked = search.rerank(fused_list(), &rerank_options(48)).await;

    assert!(!reranked.applied);
    assert_eq!(
        ids(&reranked.response.results),
        ["first", "second", "third"]
    );
    assert_eq!(reranked.response.results[0].score, 0.09);
}

#[tokio::test]
async fn reranking_scores_only_the_candidate_cap() {
    let reranker = ScriptedReranker::scoring(vec![0.0, 1.0]);
    let search = reranking_with(reranker.clone());

    let reranked = search.rerank(fused_list(), &rerank_options(2)).await;

    assert!(reranked.applied);
    assert_eq!(reranker.calls()[0].1.len(), 2);
    let third = reranked
        .response
        .results
        .iter()
        .find(|result| result.id == "third")
        .unwrap();
    assert_eq!(
        third.score, 0.07,
        "a result past the cap keeps its fused score"
    );
    assert_eq!(reranked.response.results[0].id, "second");
}

#[tokio::test]
async fn the_rerank_query_is_cut_to_its_character_budget() {
    let reranker = ScriptedReranker::scoring(vec![0.0, 0.0, 1.0]);
    let search = reranking_with(reranker.clone());
    let options = RerankOptions {
        query_max_chars: 5,
        ..rerank_options(48)
    };

    search.rerank(fused_list(), &options).await;

    assert_eq!(reranker.calls()[0].0, "which");
}

#[tokio::test]
async fn a_single_result_is_never_reranked() {
    let reranker = ScriptedReranker::scoring(vec![1.0]);
    let search = reranking_with(reranker.clone());
    let mut single = fused_list();
    single.results.truncate(1);

    let reranked = search.rerank(single, &rerank_options(48)).await;

    assert!(!reranked.applied);
    assert!(reranker.calls().is_empty());
}

#[test]
fn a_reranked_pool_is_three_times_the_cut_and_at_most_sixty_four() {
    assert_eq!(RerankOptions::candidate_pool(10), 30);
    assert_eq!(RerankOptions::candidate_pool(30), 64);
    assert_eq!(RerankOptions::candidate_pool(0), 0);
}
