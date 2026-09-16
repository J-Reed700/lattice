//! The app's real retrieval path, assembled over an in-memory corpus.
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
//!    where the production `chunks_fts_insert` trigger mirrors them into an FTS5 table.
//! 4. `HybridSearchService` runs both branches — three, with `--sparse on` — and fuses them
//!    with weighted `ReciprocalRankFusion` (k = 10 by default), then applies the shared cross-encoder
//!    blend when a reranker is supplied.
//! 5. Neighbouring-section evidence expansion mirrors `chat::retrieval::corpus_plan`.
//! 6. `chat::retrieval::assess_retrieval_sufficiency` judges the fused ranking, so a run row
//!    carries the verdict the chat pipeline would have acted on.
//!
//! `--strategy late-chunking` embeds each document in one forward pass and pools each chunk's
//! token states, mirroring `indexing::use_cases::embedding_input`. `--compression` configures
//! the USearch index's stored vector layout. `--sparse` adds the learned sparse branch, which
//! needs a model with a sparse head.
//!
//! Two deviations from production, both forced and both narrow:
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
use lattice::features::search::dto::SearchResultDto;
use lattice::features::search::engine::bm25::BM25Search;
use lattice::features::search::engine::hybrid::{
    HybridSearchResult, HybridSearchService, SearchConfig, SearchMode,
};
use lattice::features::search::engine::reranker::Reranker;
use lattice::features::search::engine::sparse_search::{
    SparseSearchService, SqliteSparseTermStore,
};
use lattice::features::search::engine::vector_search::{
    USearchVectorIndex, VectorIndexCompression,
};
use lattice::features::search::enrichment_service::SearchEnrichmentService;
use lattice::features::search::{
    BM25SearchTrait, HybridSearchTrait, SearchServiceTrait, SparseSearchTrait,
};
use lattice::features::settings::dto::RetrievalTuningSettingsDto;
use lattice::infrastructure::services::traits::SearchEnrichmentServiceTrait;
use lattice::shared::constants::MIN_SIMILARITY_SCORE;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::Document;

pub const VECTOR_BRANCH: SearchMode = SearchMode::Vector;
pub const BM25_BRANCH: SearchMode = SearchMode::Keyword;

/// Anchors whose section neighbours are pulled in, matching `corpus_plan::expand_evidence`.
const EVIDENCE_ANCHORS: usize = 8;

/// The columns the search services actually read, plus the FTS5 table and the insert trigger,
/// copied from `migrations/20260916000000_init_schema.sql`. The sparse posting table is
/// created unconditionally and simply stays empty when the branch is off.
const SCHEMA: &str = "
CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL UNIQUE,
    file_name TEXT NOT NULL,
    file_type TEXT,
    mime_type TEXT,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
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
CREATE TRIGGER chunks_fts_insert AFTER INSERT ON text_chunks BEGIN
  INSERT INTO chunks_fts(chunk_id, content)
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
    service: HybridSearchService,
    top_k: usize,
    /// Chunk id to its position inside its document, so ranking rows stay attributable
    /// without a database round trip inside the timed region.
    chunk_positions: HashMap<String, usize>,
    /// The sparse branch, kept so it can also be run on its own for the branch diagnostics.
    /// `None` when the branch is off, which is also what the run row reports.
    sparse: Option<Arc<SparseSearchService>>,
    /// Whether `HybridSearchService` applied the cross-encoder blend, which is the only
    /// condition under which the sufficiency check may read the top score as a relevance.
    reranked: bool,
}

/// Index and fusion settings for one production run.
#[derive(Debug, Clone)]
pub struct IndexOptions {
    pub compression: VectorIndexCompression,
    pub sparse_enabled: bool,
    /// Reciprocal-rank-fusion constant; the application uses 60.
    pub rrf_k: f32,
    /// Branch weights handed to `SearchConfig`; the application uses 0.7 / 0.3.
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
        for document in documents {
            let file_name = format!("{}.md", document.id);
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
                        model.embed_contextualized_chunks(&contextualized).await?,
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
        index.publish_embeddings(entries)?;

        // The same wiring `features::search::di::build` uses, minus recency and workspace scope.
        let config = SearchConfig {
            mode: SearchMode::Hybrid,
            vector_weight,
            keyword_weight,
            min_score: MIN_SIMILARITY_SCORE,
            // Direct search leaves reranking off by default; here it follows the CLI argument
            // so a reranked run exercises HybridSearchService's own blend stage.
            enable_reranking: reranker.is_some(),
            recency_boost: 1.0,
            max_results: 100,
            // `HybridSearchService` still skips the branch unless a service is
            // attached *and* the loaded model has a sparse head, so this flag
            // only says whether the run asked for it.
            sparse_enabled,
        };
        let reranked = reranker.is_some();
        let mut service = HybridSearchService::new(
            Arc::clone(&index) as Arc<dyn SearchServiceTrait>,
            Arc::new(BM25Search::new(pool.clone())) as Arc<dyn BM25SearchTrait>,
            pool.clone(),
            Arc::new(SearchEnrichmentService::new(pool.clone()))
                as Arc<dyn SearchEnrichmentServiceTrait>,
            config,
        )
        .rrf_k(rrf_k);
        if let Some(reranker) = reranker {
            service = service.with_reranker(reranker);
        }
        let sparse = sparse_enabled.then(|| {
            Arc::new(SparseSearchService::new(
                pool.clone(),
                Arc::clone(&model) as Arc<dyn EmbeddingPort>,
            ))
        });
        if let Some(sparse) = &sparse {
            service = service.with_sparse_search(Arc::clone(sparse) as Arc<dyn SparseSearchTrait>);
        }

        Ok(Self {
            pool,
            service,
            top_k,
            chunk_positions,
            sparse,
            reranked,
        })
    }

    /// Fused (and optionally reranked) chunk ranking for one query, with the chat
    /// pipeline's own verdict on whether that ranking was good enough to answer from.
    ///
    /// The verdict is taken before neighbour expansion, which is where the pipeline takes
    /// it too: neighbours are appended context, not ranking evidence.
    pub async fn rank(
        &self,
        query_text: &str,
        query_embedding: &[f32],
    ) -> Result<(Vec<RankedChunk>, RetrievalSufficiency), Box<dyn std::error::Error>> {
        let results = HybridSearchTrait::search(
            &self.service,
            query_text,
            query_embedding,
            self.top_k,
            SearchMode::Hybrid,
        )
        .await?;
        let sufficiency = assess_retrieval_sufficiency(
            &results.iter().map(sufficiency_input).collect::<Vec<_>>(),
            &[query_text.to_owned()],
            &RetrievalTuningSettingsDto::default(),
            self.reranked,
        );
        let mut ranked = Vec::with_capacity(results.len());
        for result in results {
            let chunk_index = self
                .chunk_positions
                .get(&result.chunk_id)
                .copied()
                .ok_or_else(|| format!("retrieved unknown chunk {}", result.chunk_id))?;
            ranked.push(RankedChunk {
                chunk_id: result.chunk_id,
                document_id: result.document_id,
                chunk_index,
                score: result.score,
                neighbor: false,
            });
        }
        Ok((ranked, sufficiency))
    }

    /// The learned sparse branch on its own, collapsed to its document ranking, or `None`
    /// when the branch is off. Same role as `branch_documents`: it says whether a fused
    /// regression came from this branch.
    pub async fn sparse_documents(
        &self,
        query_text: &str,
    ) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
        let Some(sparse) = self.sparse.as_ref() else {
            return Ok(None);
        };
        let results =
            SparseSearchTrait::search_scoped(sparse.as_ref(), query_text, self.top_k, None, None)
                .await?;
        let mut seen = HashSet::new();
        Ok(Some(
            results
                .into_iter()
                .filter(|result| seen.insert(result.doc_id.clone()))
                .map(|result| result.doc_id)
                .collect(),
        ))
    }

    /// One branch of the hybrid, collapsed to its document ranking, so a regression can be
    /// blamed on the vector side or the lexical side instead of on "retrieval".
    pub async fn branch_documents(
        &self,
        query_text: &str,
        query_embedding: &[f32],
        mode: SearchMode,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let results =
            HybridSearchTrait::search(&self.service, query_text, query_embedding, self.top_k, mode)
                .await?;
        let mut seen = HashSet::new();
        Ok(results
            .into_iter()
            .filter(|result| seen.insert(result.document_id.clone()))
            .map(|result| result.document_id)
            .collect())
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

/// A fused result in the shape the sufficiency check reads.
///
/// `assess_retrieval_sufficiency` consumes `score`, `title` and `content`; the title is the
/// enriched document filename, which is where `SearchResultDto` carries it everywhere else.
/// The remaining fields are copied rather than invented so the DTO stays truthful.
fn sufficiency_input(result: &HybridSearchResult) -> SearchResultDto {
    let metadata: HashMap<String, serde_json::Value> = result
        .metadata
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .map(|map| map.clone().into_iter().collect())
        .unwrap_or_default();
    let text = |key: &str| {
        metadata
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    SearchResultDto {
        id: result.id.clone(),
        title: text("filename").unwrap_or_default(),
        content: result.content.clone(),
        score: result.score,
        path: text("path"),
        document_id: Some(result.document_id.clone()),
        position: metadata
            .get("chunk_index")
            .and_then(serde_json::Value::as_u64)
            .and_then(|index| usize::try_from(index).ok()),
        vector_score: result.vector_score,
        bm25_score: result.bm25_score,
        vector_rank: result.vector_rank,
        bm25_rank: result.bm25_rank,
        metadata,
    }
}

/// `IndexingActor::process_file`'s chunk preparation, over an in-memory document.
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
            "INSERT INTO documents VALUES \
             ('d','/eval/d.md','d.md','MD','text/markdown',4,'t','t');
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

        // The exact join BM25Search and SearchEnrichmentService rely on.
        let hits: Vec<(String, String)> = sqlx::query_as(
            "SELECT tc.id, d.file_name FROM chunks_fts \
             JOIN text_chunks tc ON chunks_fts.chunk_id = tc.id \
             LEFT JOIN documents d ON tc.document_id = d.id \
             WHERE chunks_fts MATCH 'quota' ORDER BY bm25(chunks_fts)",
        )
        .fetch_all(&pool)
        .await
        .expect("fts query");
        assert_eq!(hits, [("d::0".to_owned(), "d.md".to_owned())]);
    }
}
