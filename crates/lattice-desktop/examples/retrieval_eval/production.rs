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
//!    exactly as chat's first pass searches one planned query: the vector and BM25 branches,
//!    weighted reciprocal-rank fusion (k = 10 by default), in fused order with exact repeats
//!    dropped. With a reranker, the fused pool is widened as chat widens it and passed through the
//!    same cross-encoder stage chat calls, at chat's default candidate cap and query budget.
//! 5. Neighbouring-section evidence expansion mirrors `chat::retrieval::corpus_plan`.
//! 6. `chat::retrieval::assess_retrieval_sufficiency` judges the ranking, so a run row carries
//!    the verdict the chat pipeline would have acted on.
//!
//! `--compression` configures the USearch index's stored vector layout.
//!
//! What a chat turn adds on top of this, and the harness leaves out: the corpus planner's query
//! rewrites (each query here is searched as written, as one planned query), HyDE text in the
//! rerank query, document openings and section lookups, the corrective retry, and workspace
//! scope. Two further deviations, both forced and both narrow:
//!
//! - `corpus_plan::expand_evidence` and `ConversationRepository::retrieval_neighbors` are
//!   `pub(super)` / bound to the conversation repository, so `expand` below re-implements them:
//!   anchors in rank order, same document and section, `chunk_index` within ±1, at most 3 rows
//!   per anchor, each placed after its anchor inside the evidence limit, displacing the tail.
//!   The SQL is copied from the repository.
//! - Documents are synthesised from fixture rows rather than extracted from files, so there are
//!   no page ranges and `page_number` is always NULL.

use lattice::features::conversation::chat::retrieval::{
    assess_retrieval_sufficiency, RetrievalSufficiency,
};
use lattice::features::embedding::llama_cpp::LlamaEmbeddingService;
use lattice::features::indexing::engine::chunker::{
    context_prefix, ChunkerConfig, SemanticChunker,
};
use lattice::features::indexing::engine::metadata_extractor::extract_metadata;
use lattice::features::search::engine::reranker::Reranker;
use lattice::features::search::engine::text_search::SqliteTextSearch;
use lattice::features::search::use_cases::HybridSearchUseCase;
use lattice::features::settings::dto::RetrievalTuningSettingsDto;
use lattice::infrastructure::ml::tokenizer::load_tokenizer;
use lattice::infrastructure::persistence::repositories::ChunkRepository;
use lattice::infrastructure::vector_index::{USearchVectorIndex, VectorIndexCompression};
use lattice_core::application::contracts::search::{
    BranchKind, RerankOptions, SearchModeDto, SearchRequestDto,
};
use lattice_core::application::ports::vector_search_port::VectorIndexEntry;
use lattice_core::application::ports::{EmbeddingPort, VectorSearchPort};
use lattice_core::domain::value_objects::ContextualizedChunk;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::Document;

/// The columns the search services actually read, plus the FTS5 tables, copied from
/// `migrations/20260916000000_init_schema.sql` and `20261012090000_fts_header_column.sql`.
/// `owner_conversation_id` is what a vault-wide search reads to leave chat attachments out; every
/// fixture document is a library document. The insert trigger is chosen by [`FtsHeader`].
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
    header,
    tokenize='porter unicode61 remove_diacritics 2'
);
CREATE VIRTUAL TABLE chunks_trigram USING fts5(
    chunk_id UNINDEXED,
    content,
    header,
    tokenize='trigram'
);
";

/// The production insert trigger: the context prefix in its own `header` column.
const COLUMN_TRIGGER: &str = "
CREATE TRIGGER chunks_fts_insert AFTER INSERT ON text_chunks BEGIN
  INSERT INTO chunks_fts(chunk_id, content, header)
  VALUES(new.id, new.content, new.context_prefix);
  INSERT INTO chunks_trigram(chunk_id, content, header)
  VALUES(new.id, new.content, new.context_prefix);
END;
";

/// The trigger before `20261012090000_fts_header_column.sql`: prefix and body concatenated into
/// `content`, `header` left empty. An empty column adds nothing to `bm25()` and FTS5's IDF counts
/// rows rather than columns, so this ranks exactly as the old two-column table did.
const CONCAT_TRIGGER: &str = "
CREATE TRIGGER chunks_fts_insert AFTER INSERT ON text_chunks BEGIN
  INSERT INTO chunks_fts(chunk_id, content, header)
  VALUES(new.id, COALESCE(new.contextualized_content, new.content), NULL);
  INSERT INTO chunks_trigram(chunk_id, content, header)
  VALUES(new.id, COALESCE(new.contextualized_content, new.content), NULL);
END;
";

/// Where the chunk's context prefix goes in the lexical index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FtsHeader {
    /// Its own column, weighted by `bm25()` at this weight (production).
    Column { weight: f64 },
    /// Concatenated into the body, as before the header column existed.
    Concat,
}

impl FtsHeader {
    fn trigger(self) -> &'static str {
        match self {
            FtsHeader::Column { .. } => COLUMN_TRIGGER,
            FtsHeader::Concat => CONCAT_TRIGGER,
        }
    }

    /// The row label: `"column"` or `"concat"`.
    pub fn label(self) -> &'static str {
        match self {
            FtsHeader::Column { .. } => "column",
            FtsHeader::Concat => "concat",
        }
    }

    /// The header weight `bm25()` is asked for. Irrelevant when the column is empty.
    pub fn weight(self) -> f64 {
        match self {
            FtsHeader::Column { weight } => weight,
            FtsHeader::Concat => lattice::features::search::engine::fts_query::HEADER_BM25_WEIGHT,
        }
    }
}

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

pub struct ProductionIndex {
    pool: SqlitePool,
    search: HybridSearchUseCase,
    /// Whether a reranker was supplied, which widens the fused pool before the cross-encoder
    /// stage exactly as chat widens it.
    reranks: bool,
    /// Candidates the cross-encoder scores; `None` keeps chat's default cap.
    rerank_depth: Option<usize>,
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
    /// Reciprocal-rank-fusion constant; the application uses `DEFAULT_RRF_K` (10).
    pub rrf_k: f32,
    /// Branch weights of the hybrid request; the application uses 0.7 / 0.3.
    pub vector_weight: f32,
    pub keyword_weight: f32,
    pub fts_header: FtsHeader,
    /// Candidates the cross-encoder scores, with the fused pool widened to at least that many.
    /// `None` keeps chat's default cap over the default pool.
    pub rerank_depth: Option<usize>,
}

impl ProductionIndex {
    pub async fn build(
        model: Arc<LlamaEmbeddingService>,
        tokenizer: &Path,
        documents: &[Document],
        reranker: Option<Arc<dyn Reranker>>,
        top_k: usize,
        options: IndexOptions,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let IndexOptions {
            compression,
            rrf_k,
            vector_weight,
            keyword_weight,
            fts_header,
            rerank_depth,
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
        sqlx::raw_sql(fts_header.trigger()).execute(&pool).await?;

        let tokenizer = Arc::new(load_tokenizer(tokenizer)?);
        let chunker = SemanticChunker::new(tokenizer, ChunkerConfig::default())?;
        let dimension = EmbeddingPort::dimension(&*model);
        // `VectorIndexCompression::None` is what `USearchVectorIndex::new` passes, so an
        // uncompressed run builds byte-for-byte the index it built before this flag existed.
        let index = Arc::new(USearchVectorIndex::with_compression(
            dimension,
            None,
            compression,
        )?);
        let model_identity = EmbeddingPort::model_identity(&*model);

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
            let contextualized: Vec<ContextualizedChunk> =
                prepared.iter().map(|p| p.chunk.clone()).collect();
            let embeddings = embed_cached(&model, &model_identity, &contextualized).await?;
            if embeddings.len() != prepared.len() {
                return Err(format!(
                    "document {} produced {} chunks but {} embeddings",
                    document.id,
                    prepared.len(),
                    embeddings.len()
                )
                .into());
            }
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
                entries.push(VectorIndexEntry {
                    // The DB convention the USearch index expects: emb_<chunk_id>.
                    id: format!("emb_{chunk_id}"),
                    embedding,
                    content: item.chunk.original_content,
                    chunk_id,
                    document_id: document.id.clone(),
                });
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
        // keyword index and chunk repository over the same schema, and the shared reranker. Only the embedder differs: the app wraps whichever
        // model is loaded, the harness hands in this one.
        let mut search = HybridSearchUseCase::new(
            Arc::clone(&model) as Arc<dyn EmbeddingPort>,
            Arc::clone(&index) as Arc<dyn VectorSearchPort>,
            Arc::new(SqliteTextSearch::new(pool.clone()).with_header_weight(fts_header.weight())),
        )
        .with_chunk_repository(Arc::new(ChunkRepository::new(pool.clone())))
        .with_rrf_k(rrf_k);
        let reranks = reranker.is_some();
        if let Some(reranker) = reranker {
            search = search.with_reranker(reranker);
        }

        Ok(Self {
            pool,
            search,
            reranks,
            rerank_depth,
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
                RerankOptions::candidate_pool(self.top_k).max(self.rerank_depth.unwrap_or(0))
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
    /// whole pool, before the cut to `top_k` and before neighbour expansion, which places
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
                        max_candidates: self
                            .rerank_depth
                            .unwrap_or(self.tuning.rerank_max_candidates as usize),
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
    /// regression can be blamed on the vector side or the lexical side instead of on
    /// "retrieval". Keyed `vector` and `bm25`; a branch that failed is absent.
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

    /// Place neighbouring-section evidence after each anchor, mirroring
    /// `corpus_plan::expand_evidence`: anchors in rank order, each neighbour not already
    /// evidence inserted after its anchor, the list cut back to `limit` as it goes, and the
    /// walk ended at the first anchor with no room after it.
    pub async fn expand(
        &self,
        ranked: &mut Vec<RankedChunk>,
        limit: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        ranked.truncate(limit);
        let anchors: Vec<String> = ranked.iter().map(|c| c.chunk_id.clone()).collect();
        for anchor in anchors {
            let Some(mut at) = ranked
                .iter()
                .position(|c| c.chunk_id == anchor)
                .filter(|at| at + 1 < limit)
            else {
                break;
            };
            for neighbor in self.neighbors(&anchor).await? {
                if at + 1 >= limit {
                    break;
                }
                if ranked.iter().all(|c| c.chunk_id != neighbor.chunk_id) {
                    at += 1;
                    ranked.insert(at, neighbor);
                    ranked.truncate(limit);
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

/// Chunk embeddings, read from `RETRIEVAL_EVAL_EMBED_CACHE` when that
/// names a directory. Embedding the corpus is most of a run's wall time, and a
/// change to search, fusion or the index does not alter a single vector, so a
/// cached run takes seconds. The key covers the model and the exact text, not
/// the embedding code: clear the directory after changing how text is embedded.
async fn embed_cached(
    model: &LlamaEmbeddingService,
    model_identity: &str,
    chunks: &[ContextualizedChunk],
) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    let Some(dir) = std::env::var_os("RETRIEVAL_EVAL_EMBED_CACHE").map(PathBuf::from) else {
        return Ok(embed_chunks(model, chunks).await?);
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
    let vectors = embed_chunks(model, chunks).await?;
    for (path, vector) in paths.iter().zip(&vectors) {
        let bytes: Vec<u8> = vector.iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(path, bytes)?;
    }
    Ok(vectors)
}

/// Each chunk's contextualized text, embedded in one batch as indexing sends it.
async fn embed_chunks(
    model: &LlamaEmbeddingService,
    chunks: &[ContextualizedChunk],
) -> lattice_core::shared::error::Result<Vec<Vec<f32>>> {
    let texts: Vec<String> = chunks
        .iter()
        .map(|chunk| chunk.contextualized_content.clone())
        .collect();
    model.embed_batch(&texts).await
}

/// `IndexingActor::process_file`'s chunk preparation, over an in-memory document.
fn prepare_chunks(
    model: &LlamaEmbeddingService,
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
    use lattice_core::application::ports::{ChunkRepositoryPort, TextSearchPort};

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
        sqlx::raw_sql(COLUMN_TRIGGER)
            .execute(&pool)
            .await
            .expect("trigger");
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

        // The trigger must split prefix and body the way the migration does, since BM25
        // weighs the two columns apart.
        let indexed: (String, String) =
            sqlx::query_as("SELECT content, header FROM chunks_fts WHERE chunk_id = 'd::0'")
                .fetch_one(&pool)
                .await
                .expect("trigger populated chunks_fts");
        assert_eq!(indexed, ("quota".to_owned(), "[Document: d]".to_owned()));

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
