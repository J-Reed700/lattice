//! Chat's library search, assembled over an in-memory corpus.
//!
//! Everything here is production code from the `lattice` crate; this module only supplies the
//! storage the services expect. In order:
//!
//! 1. `SemanticChunker` (800 tokens / 120 overlap) splits a fixture document, exactly as the
//!    indexing engine's chunker does.
//! 2. `metadata_extractor::extract_metadata` derives the section, and `chunker::context_prefix`
//!    builds the `[Document: … | Section: …]` prefix. Each chunk is then re-split through the
//!    embedder's own input policy with that prefix, and the prefixed text is what gets embedded —
//!    the same sequence `IndexingActor::process_file` runs.
//! 3. Vectors go into a `USearchVectorIndex` (HNSW, cosine, f32); chunk rows go into SQLite,
//!    where the production `chunks_fts_insert` trigger mirrors them into both FTS5 tables.
//! 4. `HybridSearchUseCase` — the orchestrator chat, the tool executor and the search page run —
//!    is composed the way `features::search::di::build` composes it, and searches each query
//!    exactly as chat's first pass searches one planned query: the vector and BM25 branches (and
//!    the learned sparse one, with `--sparse on`), weighted reciprocal-rank fusion (k = 10 by
//!    default) with chat's per-document cap. With a reranker, the fused pool is widened as chat
//!    widens it and passed through the same cross-encoder stage chat calls, at chat's default
//!    candidate cap and query budget.
//! 5. Neighbouring-section evidence expansion mirrors `chat::retrieval::corpus_plan`.
//! 6. `chat::retrieval::assess_retrieval_sufficiency` judges the ranking, so a run row carries
//!    the verdict the chat pipeline would have acted on.
//!
//! `--strategy late-chunking` embeds each document in one forward pass and pools each chunk's
//! token states, mirroring `indexing::use_cases::embedding_input`. `--compression` configures
//! the USearch index's stored vector layout. `--sparse` adds the learned sparse branch, which
//! needs a model with a sparse head.
//!
//! What a chat turn adds on top of this, and the harness leaves out: the corpus planner's query
//! rewrites (each query here is searched as written, as one planned query), HyDE text in the
//! rerank query, document openings and section lookups, the corrective retry, and workspace
//! scope. Two further deviations, both forced and both narrow:
//!
//! - `corpus_plan::expand_evidence` and `ConversationRepository::retrieval_neighbors` are
//!   `pub(super)` / bound to the conversation repository, so `expand` below re-implements them:
//!   8 anchors, same document and section, `chunk_index` within ±1, at most 3 rows per anchor,
//!   stopping at the evidence limit. The SQL is copied from the repository.
//! - Documents are synthesised from fixture rows rather than extracted from files, so there are
//!   no page ranges and `page_number` is always NULL.

use lattice::application::ports::vector_search_port::VectorIndexEntry;
use lattice::application::ports::{
    ChunkSparseTerms, EmbeddingPort, SparseTermStorePort, VectorSearchPort,
};
use lattice::features::conversation::chat::retrieval::{
    assess_retrieval_sufficiency, RetrievalSufficiency,
};
use lattice::features::embedding::candle_service::CandleEmbeddingService;
use lattice::features::embedding::input_policy::load_tokenizer;
use lattice::features::indexing::engine::chunker::{
    context_prefix, ChunkerConfig, ContextualizedChunk, SemanticChunker,
};
use lattice::features::indexing::engine::metadata_extractor::extract_metadata;
use lattice::features::search::dto::{SearchModeDto, SearchRequestDto};
use lattice::features::search::engine::reranker::Reranker;
use lattice::features::search::engine::sparse_search::{
    SparseSearchService, SqliteSparseTermStore,
};
use lattice::features::search::engine::text_search::SqliteTextSearch;
use lattice::features::search::engine::vector_search::{
    USearchVectorIndex, VectorIndexCompression,
};
use lattice::features::search::use_cases::{BranchKind, HybridSearchUseCase, RerankOptions};
use lattice::features::search::SparseSearchTrait;
use lattice::features::settings::dto::RetrievalTuningSettingsDto;
use lattice::infrastructure::persistence::repositories::ChunkRepository;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::Document;

/// Anchors whose section neighbours are pulled in, matching `corpus_plan::expand_evidence`.
const EVIDENCE_ANCHORS: usize = 8;

/// The columns the search services actually read, plus the FTS5 table and the insert trigger,
/// copied from `migrations/20260916000000_init_schema.sql`. `owner_conversation_id` is what a
/// vault-wide search reads to leave chat attachments out; every fixture document is a library
/// document. The sparse posting table is created unconditionally and simply stays empty when the
/// branch is off.
const SCHEMA: &str = "
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL UNIQUE,
    file_name TEXT NOT NULL,
    file_type TEXT,
    mime_type TEXT,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    owner_conversation_id TEXT
);
CREATE TABLE text_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    content TEXT NOT NULL,
    contextualized_content TEXT,
    context_prefix TEXT,
    chunk_index INTEGER NOT NULL,
    start_char INTEGER,
    end_char INTEGER,
    section TEXT,
    page_number INTEGER,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);
CREATE INDEX idx_chunk_section_order ON text_chunks(document_id, section, chunk_index);
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    chunk_id UNINDEXED,
    content,
    tokenize='porter unicode61 remove_diacritics 2'
);
CREATE VIRTUAL TABLE chunks_trigram USING fts5(
    chunk_id UNINDEXED,
    content,
    tokenize='trigram'
);
CREATE TRIGGER chunks_fts_insert AFTER INSERT ON text_chunks BEGIN
  INSERT INTO chunks_fts(chunk_id, content)
  VALUES(new.id, COALESCE(new.contextualized_content, new.content));
  INSERT INTO chunks_trigram(chunk_id, content)
  VALUES(new.id, COALESCE(new.contextualized_content, new.content));
END;
CREATE TABLE IF NOT EXISTS chunk_sparse_terms (
    chunk_id TEXT NOT NULL REFERENCES text_chunks(id) ON DELETE CASCADE,
    model_identity TEXT NOT NULL,
    term_id INTEGER NOT NULL,
    weight REAL NOT NULL,
    PRIMARY KEY (chunk_id, model_identity, term_id)
);
CREATE INDEX IF NOT EXISTS idx_chunk_sparse_terms_lookup
    ON chunk_sparse_terms(model_identity, term_id);
";

/// One retrieved chunk, carrying enough identity to attribute a document hit to a passage.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedChunk {
    pub chunk_id: String,
    pub document_id: String,
    pub chunk_index: usize,
    pub score: f32,
    /// True when the chunk entered through section-neighbour expansion rather than ranking.
    pub neighbor: bool,
}

impl RankedChunk {
    /// Stable `document#chunk` locator for the run rows.
    pub fn locator(&self) -> String {
        format!("{}#{}", self.document_id, self.chunk_index)
    }
}

/// A chunk prepared exactly as the indexer prepares one, plus the section it was filed under.
struct PreparedChunk {
    chunk: ContextualizedChunk,
    section: Option<String>,
}

/// One document's late-chunking input: the span text and the byte range of each of its
/// chunks inside it, exactly the pair `EmbeddingPort::embed_span_chunks` expects.
struct SpanGroup {
    span_text: String,
    chunk_ranges: Vec<Range<usize>>,
}

pub struct ProductionIndex {
    pool: SqlitePool,
    search: HybridSearchUseCase,
    /// Whether a reranker was supplied, which widens the fused pool before the cross-encoder
    /// stage exactly as chat widens it.
    reranks: bool,
    vector_weight: f32,
    keyword_weight: f32,
    /// Chat's retrieval tuning defaults: the rerank candidate cap, the rerank query budget and
    /// the sufficiency thresholds.
    tuning: RetrievalTuningSettingsDto,
    top_k: usize,
    /// Document id to its file name, which chat uses as every passage's title. The title is
    /// part of what the cross-encoder reads and what the sufficiency check reads.
    document_names: HashMap<String, String>,
    /// Chunk id to its position inside its document, so ranking rows stay attributable
    /// without a database round trip inside the timed region.
    chunk_positions: HashMap<String, usize>,
    /// `document#chunk` locator to the UTF-8 byte range the chunk covers in its document.
    chunk_spans: HashMap<String, (usize, usize)>,
}

/// One query's ranking and the chat pipeline's verdict on it.
pub struct Ranking {
    pub chunks: Vec<RankedChunk>,
    pub sufficiency: RetrievalSufficiency,
    /// Whether the cross-encoder stage applied its blend, which is the only condition under
    /// which the sufficiency check may read the top score as a relevance.
    pub reranked: bool,
}

/// Index and fusion settings for one production run.
#[derive(Debug, Clone)]
pub struct IndexOptions {
    pub compression: VectorIndexCompression,
    pub sparse_enabled: bool,
    /// Reciprocal-rank-fusion constant; the application uses `DEFAULT_RRF_K` (10).
    pub rrf_k: f32,
    /// Branch weights of the hybrid request; the application uses 0.7 / 0.3.
    pub vector_weight: f32,
    pub keyword_weight: f32,
}

impl ProductionIndex {
    pub async fn build(
        model: Arc<CandleEmbeddingService>,
        embedding_dir: &Path,
        documents: &[Document],
        reranker: Option<Arc<dyn Reranker>>,
        top_k: usize,
        options: IndexOptions,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let IndexOptions {
            compression,
            sparse_enabled,
            rrf_k,
            vector_weight,
            keyword_weight,
        } = options;
        // An in-memory database only exists for as long as its one connection does, and a
        // full evaluation run outlives sqlx's default idle and lifetime reaping, so the
        // connection is pinned open rather than recycled out from under the corpus.
        let pool = SqlitePoolOptions::new()
            .min_connections(1)
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect(":memory:")
            .await?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;

        let tokenizer = Arc::new(load_tokenizer(&embedding_dir.join("tokenizer.json"))?);
        let chunker = SemanticChunker::new(tokenizer, ChunkerConfig::default())?;
        let dimension = EmbeddingPort::dimension(&*model);
        // `VectorIndexCompression::None` is what `USearchVectorIndex::new` passes, so an
        // uncompressed run builds byte-for-byte the index it built before this flag existed.
        let index = Arc::new(USearchVectorIndex::with_compression(
            dimension,
            None,
            compression,
        )?);
        let late_chunking = EmbeddingPort::uses_late_chunking(&*model);
        let model_identity = EmbeddingPort::model_identity(&*model);
        let sparse_store = SqliteSparseTermStore::new(pool.clone());

        let mut entries = Vec::new();
        let mut chunk_positions = HashMap::new();
        let mut chunk_spans = HashMap::new();
        let mut document_names = HashMap::new();
        for document in documents {
            let file_name = format!("{}.md", document.id);
            document_names.insert(document.id.clone(), file_name.clone());
            let title = document.title.clone().unwrap_or_else(|| file_name.clone());
            sqlx::query(
                "INSERT INTO documents \
                 (id, file_path, file_name, file_type, mime_type, size_bytes, created_at, updated_at) \
                 VALUES (?, ?, ?, 'MD', 'text/markdown', ?, '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z')",
            )
            .bind(&document.id)
            .bind(format!("/eval/{file_name}"))
            .bind(&file_name)
            .bind(document.text.len() as i64)
            .execute(&pool)
            .await?;

            let prepared = prepare_chunks(&model, &chunker, &title, &file_name, &document.text)?;
            // The texts `IndexingActor` hands the embedder: prefix plus chunk, once each.
            let contextualized_texts: Vec<String> = prepared
                .iter()
                .map(|p| p.chunk.contextualized_content.clone())
                .collect();
            // The same split `index_file::index` makes. Chunk-first gets both representations
            // from one forward pass; late chunking pools dense vectors over a whole span and
            // has no equivalent pooling for term weights, so the sparse head reads the
            // per-chunk texts in a second pass.
            let (embeddings, sparse_terms) = match (sparse_enabled, late_chunking) {
                (false, false) => {
                    let contextualized: Vec<ContextualizedChunk> =
                        prepared.iter().map(|p| p.chunk.clone()).collect();
                    (
                        embed_cached(&model, &model_identity, &contextualized).await?,
                        Vec::new(),
                    )
                }
                (false, true) => (
                    embed_spans(&model, &prepared, &document.text).await?,
                    Vec::new(),
                ),
                (true, false) => {
                    EmbeddingPort::embed_batch_with_sparse(&*model, &contextualized_texts).await?
                }
                (true, true) => (
                    embed_spans(&model, &prepared, &document.text).await?,
                    EmbeddingPort::embed_sparse_batch(&*model, &contextualized_texts).await?,
                ),
            };
            if embeddings.len() != prepared.len() {
                return Err(format!(
                    "document {} produced {} chunks but {} embeddings",
                    document.id,
                    prepared.len(),
                    embeddings.len()
                )
                .into());
            }
            // A short sparse batch would key one chunk's terms to another chunk's id, which
            // is worse than having no sparse rows at all. `index_file` warns and drops; an
            // evaluation run must not quietly measure a corrupted index.
            if sparse_enabled && sparse_terms.len() != prepared.len() {
                return Err(format!(
                    "document {} produced {} chunks but {} sparse term vectors",
                    document.id,
                    prepared.len(),
                    sparse_terms.len()
                )
                .into());
            }
            let mut sparse_entries: Vec<ChunkSparseTerms> = Vec::new();
            for (item, embedding) in prepared.into_iter().zip(embeddings) {
                let chunk_id = format!("{}::{}", document.id, item.chunk.chunk_index);
                sqlx::query(
                    "INSERT INTO text_chunks \
                     (id, document_id, content, contextualized_content, context_prefix, \
                      chunk_index, start_char, end_char, section) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&chunk_id)
                .bind(&document.id)
                .bind(&item.chunk.original_content)
                .bind(&item.chunk.contextualized_content)
                .bind(&item.chunk.context_prefix)
                .bind(item.chunk.chunk_index as i64)
                .bind(item.chunk.start_idx as i64)
                .bind(item.chunk.end_idx as i64)
                .bind(item.section.as_deref())
                .execute(&pool)
                .await?;

                chunk_positions.insert(chunk_id.clone(), item.chunk.chunk_index);
                chunk_spans.insert(
                    format!("{}#{}", document.id, item.chunk.chunk_index),
                    (item.chunk.start_idx, item.chunk.end_idx),
                );
                if let Some(sparse) = sparse_terms.get(item.chunk.chunk_index) {
                    sparse_entries.push((chunk_id.clone(), sparse.clone()));
                }
                entries.push(VectorIndexEntry {
                    // The DB convention the USearch index expects: emb_<chunk_id>.
                    id: format!("emb_{chunk_id}"),
                    embedding,
                    content: item.chunk.original_content,
                    chunk_id,
                    document_id: document.id.clone(),
                });
            }
            if !sparse_entries.is_empty() {
                sparse_store
                    .replace_chunk_terms(&model_identity, &sparse_entries)
                    .await?;
            }
        }
        if entries.is_empty() {
            return Err("the fixture produced no indexable chunks".into());
        }
        // Chunk size is a retrieval knob now, so the corpus's chunk count is
        // part of what a run reports. stdout carries the JSONL rows.
        eprintln!(
            "indexed {} chunks from {} documents",
            entries.len(),
            documents.len()
        );
        index.publish_embeddings(entries)?;

        // Composed as `features::search::di::build` composes the application's: the same
        // keyword index and chunk repository over the same schema, the sparse branch behind its
        // switch, and the shared reranker. Only the embedder differs: the app wraps whichever
        // model is loaded, the harness hands in this one.
        let mut search = HybridSearchUseCase::new(
            Arc::clone(&model) as Arc<dyn EmbeddingPort>,
            Arc::clone(&index) as Arc<dyn VectorSearchPort>,
            Arc::new(SqliteTextSearch::new(pool.clone())),
        )
        .with_chunk_repository(Arc::new(ChunkRepository::new(pool.clone())))
        .with_rrf_k(rrf_k);
        if sparse_enabled {
            search = search.with_sparse_search(
                Arc::new(SparseSearchService::new(
                    pool.clone(),
                    Arc::clone(&model) as Arc<dyn EmbeddingPort>,
                )) as Arc<dyn SparseSearchTrait>,
                true,
            );
        }
        let reranks = reranker.is_some();
        if let Some(reranker) = reranker {
            search = search.with_reranker(reranker);
        }

        Ok(Self {
            pool,
            search,
            reranks,
            vector_weight,
            keyword_weight,
            tuning: RetrievalTuningSettingsDto::default(),
            top_k,
            document_names,
            chunk_positions,
            chunk_spans,
        })
    }

    /// The hybrid request chat sends for one planned query, at this run's fusion weights. With
    /// a reranker the pool is widened as chat widens it, so the cross-encoder can promote a
    /// passage fusion left just outside the cut.
    fn request(&self, query_text: &str) -> SearchRequestDto {
        SearchRequestDto {
            query: query_text.to_owned(),
            limit: Some(if self.reranks {
                RerankOptions::candidate_pool(self.top_k)
            } else {
                self.top_k
            }),
            threshold: None,
            mode: SearchModeDto::Hybrid {
                vector_weight: self.vector_weight,
                bm25_weight: self.keyword_weight,
            },
        }
    }

    /// Fused (and optionally reranked) chunk ranking for one query, with the chat
    /// pipeline's own verdict on whether that ranking was good enough to answer from.
    ///
    /// The verdict is taken where the pipeline takes it: after the rerank stage, over the
    /// whole pool, before the cut to `top_k` and before neighbour expansion, which appends
    /// context rather than ranking evidence.
    pub async fn rank(&self, query_text: &str) -> Result<Ranking, Box<dyn std::error::Error>> {
        let mut response = self.search.execute(self.request(query_text)).await?;
        // Chat titles every passage with its document's file name before reranking.
        for result in &mut response.results {
            if let Some(name) = result
                .document_id
                .as_ref()
                .and_then(|id| self.document_names.get(id))
            {
                result.title.clone_from(name);
            }
        }
        let mut reranked = false;
        if self.reranks {
            let stage = self
                .search
                .rerank(
                    response,
                    &RerankOptions {
                        query: query_text.to_owned(),
                        max_candidates: self.tuning.rerank_max_candidates as usize,
                        query_max_chars: self.tuning.rerank_query_max_chars as usize,
                    },
                )
                .await;
            response = stage.response;
            reranked = stage.applied;
        }
        let sufficiency = assess_retrieval_sufficiency(
            &response.results,
            &[query_text.to_owned()],
            &self.tuning,
            reranked,
        );
        response.results.truncate(self.top_k);
        let mut chunks = Vec::with_capacity(response.results.len());
        for result in response.results {
            let chunk_index = self
                .chunk_positions
                .get(&result.id)
                .copied()
                .ok_or_else(|| format!("retrieved unknown chunk {}", result.id))?;
            chunks.push(RankedChunk {
                document_id: result
                    .document_id
                    .ok_or_else(|| format!("chunk {} has no document", result.id))?,
                chunk_id: result.id,
                chunk_index,
                score: result.score,
                neighbor: false,
            });
        }
        Ok(Ranking {
            chunks,
            sufficiency,
            reranked,
        })
    }

    /// Byte ranges for the given chunks, keyed by locator, so the scorer can tell whether the
    /// retrieved chunk held the answer passage and not merely the right document.
    pub fn spans_for(&self, chunks: &[RankedChunk]) -> HashMap<String, [usize; 2]> {
        chunks
            .iter()
            .filter_map(|chunk| {
                let locator = chunk.locator();
                let (start, end) = self.chunk_spans.get(&locator).copied()?;
                Some((locator, [start, end]))
            })
            .collect()
    }

    /// Each branch the fusion saw for this query, collapsed to its document ranking, so a
    /// regression can be blamed on the vector side, the lexical side or the sparse side instead
    /// of on "retrieval". Keyed `vector`, `bm25` and, when that branch ran, `sparse`; a branch
    /// that failed is absent.
    pub async fn branch_documents(
        &self,
        query_text: &str,
    ) -> serde_json::Map<String, serde_json::Value> {
        let request = self.request(query_text);
        let query = self
            .search
            .search_branches(
                query_text,
                None,
                None,
                request.limit.unwrap_or(self.top_k),
                self.vector_weight,
                self.keyword_weight,
            )
            .await;
        let mut branches = serde_json::Map::new();
        for branch in query.branches {
            let key = match branch.kind {
                BranchKind::Vector => "vector",
                BranchKind::Lexical => "bm25",
                BranchKind::Sparse => "sparse",
                BranchKind::Fused => continue,
            };
            let mut seen = HashSet::new();
            let documents: Vec<String> = branch
                .results
                .into_iter()
                .filter_map(|result| result.document_id)
                .filter(|id| seen.insert(id.clone()))
                .collect();
            branches.insert(key.to_owned(), serde_json::json!(documents));
        }
        branches
    }

    /// Append neighbouring-section evidence, mirroring `corpus_plan::expand_evidence`.
    pub async fn expand(
        &self,
        ranked: &mut Vec<RankedChunk>,
        limit: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let anchors: Vec<String> = ranked
            .iter()
            .take(EVIDENCE_ANCHORS)
            .map(|chunk| chunk.chunk_id.clone())
            .collect();
        let mut seen: HashSet<String> = ranked.iter().map(|c| c.chunk_id.clone()).collect();
        for anchor in anchors {
            for neighbor in self.neighbors(&anchor).await? {
                if ranked.len() >= limit {
                    return Ok(());
                }
                if seen.insert(neighbor.chunk_id.clone()) {
                    ranked.push(neighbor);
                }
            }
        }
        Ok(())
    }

    /// The query from `ConversationRepository::retrieval_neighbors`, without workspace scoping.
    async fn neighbors(&self, chunk_id: &str) -> Result<Vec<RankedChunk>, sqlx::Error> {
        let anchor: Option<(String, i64, Option<String>)> = sqlx::query_as(
            "SELECT document_id, chunk_index, section FROM text_chunks WHERE id = ?",
        )
        .bind(chunk_id)
        .fetch_optional(&self.pool)
        .await?;
        // A chunk with no section has no defined neighbourhood; production returns nothing.
        let Some((document_id, index, Some(section))) = anchor else {
            return Ok(Vec::new());
        };
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT c.id, c.chunk_index FROM text_chunks c \
             WHERE c.document_id = ? AND c.section = ? AND c.chunk_index BETWEEN ? AND ? \
             ORDER BY c.chunk_index LIMIT 3",
        )
        .bind(&document_id)
        .bind(&section)
        .bind(index.saturating_sub(1))
        .bind(index + 1)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(chunk_id, chunk_index)| RankedChunk {
                chunk_id,
                document_id: document_id.clone(),
                chunk_index: chunk_index.max(0) as usize,
                // Neighbours are appended evidence, not ranked hits; production gives them no
                // fusion score either. Keeping them at zero stops them inflating `top_score`.
                score: 0.0,
                neighbor: true,
            })
            .collect())
    }
}

/// Group prepared chunks into late-chunking spans, mirroring
/// `indexing::use_cases::embedding_input::prepare_structured_with_spans`: the span text is
/// the passage with its context prefix prepended exactly once, and each range addresses a
/// chunk's bytes inside that span, so the prefix conditions the pass without ever being
/// pooled into a chunk.
///
/// Here the passage is the whole document, because `prepare_chunks` records `start_idx` and
/// `end_idx` against the document text; the ranges are those offsets shifted by the prefix.
/// A document whose chunks were filed under different sections carries more than one prefix,
/// and each prefix opens its own span rather than embedding a chunk under a neighbour's
/// heading. On these fixtures a document almost always has exactly one.
fn span_groups(prepared: &[PreparedChunk], text: &str) -> Vec<SpanGroup> {
    let mut groups: Vec<SpanGroup> = Vec::new();
    let mut current: Option<&str> = None;
    for item in prepared {
        let prefix = item.chunk.context_prefix.as_str();
        if current != Some(prefix) {
            groups.push(SpanGroup {
                span_text: format!("{prefix}\n\n{text}"),
                chunk_ranges: Vec::new(),
            });
            current = Some(prefix);
        }
        // `prepare_chunks` splits with `format!("{prefix}\n\n")`, so that is the prefix
        // whose length the chunk offsets shift by.
        let offset = prefix.len() + 2;
        if let Some(group) = groups.last_mut() {
            group
                .chunk_ranges
                .push(offset + item.chunk.start_idx..offset + item.chunk.end_idx);
        }
    }
    groups
}

/// One vector per prepared chunk, in chunk order, from the late-chunking path.
///
/// `embed_span_chunks` falls back to per-chunk embedding internally when a span does not fit
/// the model window, so a long document still yields a full set of vectors.
async fn embed_spans(
    model: &CandleEmbeddingService,
    prepared: &[PreparedChunk],
    text: &str,
) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
    let mut vectors = Vec::with_capacity(prepared.len());
    for group in span_groups(prepared, text) {
        vectors.extend(
            EmbeddingPort::embed_span_chunks(model, &group.span_text, &group.chunk_ranges).await?,
        );
    }
    Ok(vectors)
}

/// `IndexingActor::process_file`'s chunk preparation, over an in-memory document.
/// Chunk-first embeddings, read from `RETRIEVAL_EVAL_EMBED_CACHE` when that
/// names a directory. Embedding the corpus is most of a run's wall time, and a
/// change to search, fusion or the index does not alter a single vector, so a
/// cached run takes seconds. The key covers the model and the exact text, not
/// the embedding code: clear the directory after changing how text is embedded.
async fn embed_cached(
    model: &CandleEmbeddingService,
    model_identity: &str,
    chunks: &[ContextualizedChunk],
) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    let Some(dir) = std::env::var_os("RETRIEVAL_EVAL_EMBED_CACHE").map(PathBuf::from) else {
        return Ok(model.embed_contextualized_chunks(chunks).await?);
    };
    std::fs::create_dir_all(&dir)?;
    let paths: Vec<PathBuf> = chunks
        .iter()
        .map(|chunk| {
            let mut hasher = Sha256::new();
            hasher.update(model_identity.as_bytes());
            hasher.update([0]);
            hasher.update(chunk.contextualized_content.as_bytes());
            dir.join(format!("{}.f32", hex::encode(hasher.finalize())))
        })
        .collect();
    let cached: Option<Vec<Vec<f32>>> = paths
        .iter()
        .map(|path| {
            let bytes = std::fs::read(path).ok()?;
            (bytes.len() == model.dimension() * 4).then(|| {
                bytes
                    .chunks_exact(4)
                    .filter_map(|b| <[u8; 4]>::try_from(b).ok())
                    .map(f32::from_le_bytes)
                    .collect()
            })
        })
        .collect();
    if let Some(vectors) = cached {
        return Ok(vectors);
    }
    let vectors = model.embed_contextualized_chunks(chunks).await?;
    for (path, vector) in paths.iter().zip(&vectors) {
        let bytes: Vec<u8> = vector.iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(path, bytes)?;
    }
    Ok(vectors)
}

fn prepare_chunks(
    model: &CandleEmbeddingService,
    chunker: &SemanticChunker,
    title: &str,
    file_name: &str,
    text: &str,
) -> Result<Vec<PreparedChunk>, Box<dyn std::error::Error>> {
    let path = PathBuf::from(file_name);
    let mut base_metadata = extract_metadata(&path, text, 0)?;
    // The fixture's own title stands in for the file name a real import would carry.
    base_metadata.title = title.to_owned();
    let mut prepared: Vec<PreparedChunk> = Vec::new();
    for chunk in chunker.chunk_text(text)? {
        let mut metadata = base_metadata.clone();
        if metadata.section.is_none() {
            metadata.section = extract_metadata(&path, &chunk.text, 0)?.section;
        }
        let prefix = context_prefix(&metadata);
        for part in model.split_text(&chunk.text, &format!("{prefix}\n\n"))? {
            prepared.push(PreparedChunk {
                chunk: ContextualizedChunk {
                    contextualized_content: format!("{prefix}\n\n{}", part.text),
                    original_content: part.text,
                    context_prefix: prefix.clone(),
                    chunk_index: prepared.len(),
                    token_count: part.token_count,
                    start_idx: chunk.start_idx + part.start,
                    end_idx: chunk.start_idx + part.end,
                },
                section: metadata.section.clone(),
            });
        }
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice::application::ports::{ChunkRepositoryPort, TextSearchPort};

    fn chunk(document_id: &str, chunk_index: usize, score: f32) -> RankedChunk {
        RankedChunk {
            chunk_id: format!("{document_id}::{chunk_index}"),
            document_id: document_id.to_owned(),
            chunk_index,
            score,
            neighbor: false,
        }
    }

    #[test]
    fn locator_names_the_document_and_the_passage() {
        assert_eq!(
            chunk("ops-rate-limit", 3, 0.02).locator(),
            "ops-rate-limit#3"
        );
    }

    #[tokio::test]
    async fn schema_matches_what_the_search_services_query() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("in-memory pool");
        sqlx::raw_sql(SCHEMA).execute(&pool).await.expect("schema");
        sqlx::raw_sql(
            "INSERT INTO documents \
             (id, file_path, file_name, file_type, mime_type, size_bytes, created_at, updated_at) \
             VALUES ('d','/eval/d.md','d.md','MD','text/markdown',4,'t','t');
             INSERT INTO text_chunks \
             (id, document_id, content, contextualized_content, context_prefix, chunk_index, \
              start_char, end_char, section) \
             VALUES ('d::0','d','quota','[Document: d] quota','[Document: d]',0,0,5,'Limits');",
        )
        .execute(&pool)
        .await
        .expect("seed rows");

        // The trigger must mirror the contextualized text, which is what BM25 ranks.
        let indexed: String =
            sqlx::query_scalar("SELECT content FROM chunks_fts WHERE chunk_id = 'd::0'")
                .fetch_one(&pool)
                .await
                .expect("trigger populated chunks_fts");
        assert_eq!(indexed, "[Document: d] quota");

        // What the orchestrator's vault-wide branches read: the keyword search, which leaves
        // chat attachments out, and the chunk repository's attachment and body lookups.
        let hits = TextSearchPort::search_scoped(
            &SqliteTextSearch::new(pool.clone()),
            "quota",
            5,
            None,
            None,
        )
        .await
        .expect("keyword search");
        assert_eq!(hits.len(), 1);
        assert_eq!(
            (hits[0].chunk_id.as_str(), hits[0].doc_id.as_str()),
            ("d::0", "d")
        );
        let chunks = ChunkRepository::new(pool);
        assert!(
            ChunkRepositoryPort::find_conversation_attached_chunk_ids(&chunks)
                .await
                .expect("attachment lookup")
                .is_empty()
        );
        let bodies = ChunkRepositoryPort::find_content_by_ids(&chunks, &["d::0".to_owned()])
            .await
            .expect("body lookup");
        assert_eq!(bodies.get("d::0").map(String::as_str), Some("quota"));
    }
}
