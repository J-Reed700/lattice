//! Indexing, search and retrieval-tuning settings and their defaults.

use serde::{Deserialize, Serialize};

/// Indexing settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexingSettingsDto {
    /// Chunk size for text splitting
    pub chunk_size: u32,

    /// Overlap between chunks
    pub chunk_overlap: u32,

    /// Batch size for processing
    pub batch_size: u32,

    /// Automatically index new files
    pub auto_index_new_files: bool,

    /// Supported file types
    pub file_types: Vec<String>,

    /// Folders being watched / indexed (the user's watch list).
    /// Mutated only via the dedicated `add_watch_folder` / `remove_watch_folder`
    /// commands so backend can run path validation (CWE-22 / CWE-158).
    #[serde(default)]
    pub indexed_paths: Vec<String>,

    /// File patterns to ignore during indexing (e.g. `*.tmp`, `node_modules`).
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

/// Search settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchSettingsDto {
    /// Maximum number of results to return
    pub max_results: u32,

    /// Similarity threshold (0.0 - 1.0)
    pub similarity_threshold: f32,

    /// Enable result reranking
    pub enable_reranking: bool,

    /// Hybrid search alpha parameter (0.0 = vector, 1.0 = keyword)
    pub hybrid_search_alpha: f32,

    /// Retrieval pipeline tuning knobs (overlap/rerank/shortlist/web+wiki shaping)
    #[serde(default)]
    pub retrieval_tuning: RetrievalTuningSettingsDto,

    /// How vectors are stored in the search index.
    ///
    /// Lives next to the retrieval knobs because it is a property of the
    /// search index, not of a document: changing it renames the index file and
    /// the next start rebuilds it from the f32 vectors SQLite already holds.
    #[serde(default)]
    pub vector_index_compression: VectorIndexCompressionSettingsDto,

    /// How chunk vectors are produced at index time.
    ///
    /// Also a property of the vector space rather than of one document — the
    /// two strategies are not interchangeable, so flipping it changes the
    /// embedding generation and re-embeds the corpus.
    #[serde(default)]
    pub embedding_strategy: EmbeddingStrategySettingDto,

    /// Generate and search document/section summaries as a collection-level
    /// retrieval tier. Summaries are produced off the indexing critical path
    /// by the utility model, so this is off until a utility model is set up.
    #[serde(default)]
    pub summary_index_enabled: bool,
}

/// Whether the search index stores full-precision vectors or a compressed
/// projection of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum VectorIndexCompressionModeDto {
    /// Store the embedding as produced: full dimension, `f32`.
    #[default]
    None,
    /// Store a renormalized leading prefix of the embedding, optionally
    /// quantized, and rescore over-fetched candidates against full vectors.
    /// Only meaningful for models trained with Matryoshka Representation
    /// Learning; the composition root ignores it for models that are not.
    Truncated,
}

/// Scalar type the index stores each vector component as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum VectorQuantizationDto {
    /// No quantization loss, 4 bytes per component.
    #[default]
    F32,
    /// 1 byte per component; the index becomes a candidate generator and
    /// searches rescore against full-precision vectors.
    I8,
}

/// Vector storage configuration for the search index.
///
/// `dims` and `quantization` are remembered even while `mode` is `None`, so a
/// user who turns truncation off and on again gets their own numbers back
/// rather than the defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VectorIndexCompressionSettingsDto {
    pub mode: VectorIndexCompressionModeDto,
    /// Leading components kept when `mode` is `truncated`. Clamped against the
    /// active model's real dimension where the index is opened.
    pub dims: u32,
    pub quantization: VectorQuantizationDto,
}

/// How chunk vectors are produced at index time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingStrategySettingDto {
    /// Embed each chunk separately with its context prefix prepended.
    #[default]
    ChunkFirst,
    /// Embed the whole structure span once and mean-pool each chunk's token
    /// states, so every chunk is conditioned on the rest of its span.
    LateChunking,
}

/// Retrieval pipeline tuning settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalTuningSettingsDto {
    /// Min and max KB candidate limits derived from tool output max_results
    pub kb_search_min_limit: u32,
    pub kb_search_max_limit: u32,

    /// Min and max limits for document shortlist candidate collection
    pub doc_shortlist_candidate_min: u32,
    pub doc_shortlist_candidate_max: u32,

    /// Min and max document count kept in document shortlist
    pub doc_shortlist_doc_min: u32,
    pub doc_shortlist_doc_max: u32,

    /// Minimum shortlist confidence required before gating
    pub shortlist_gate_min_candidates: u32,
    pub shortlist_gate_min_docs: u32,

    /// External source retrieval limits and snippet shaping
    pub wiki_search_max_results: u32,
    pub wiki_snippet_max_chars: u32,
    pub wiki_context_limit: u32,
    pub web_search_max_results: u32,
    pub web_snippet_max_chars: u32,
    /// How many of the top web results are opened and read in full for the
    /// prompt. Search engines return a headline and roughly one sentence; a
    /// question about what a page actually says cannot be answered from that.
    /// 0 disables page reading and falls back to snippets alone.
    #[serde(default = "default_web_fetch_page_count")]
    pub web_fetch_page_count: u32,
    /// Ceiling on any one fetched page's text in the prompt. The room a page
    /// actually gets is its share of the turn's page budget, which is derived
    /// from the model's context window; this only caps that share.
    #[serde(default = "default_web_page_max_chars")]
    pub web_page_max_chars: u32,
    /// Seconds any one page fetch may take before it is abandoned and the
    /// result falls back to its snippet.
    #[serde(default = "default_web_page_fetch_timeout_secs")]
    pub web_page_fetch_timeout_secs: u32,
    #[serde(default = "default_deep_research_depth")]
    pub deep_research_depth: u32,
    #[serde(default = "default_deep_research_branch_queries")]
    pub deep_research_branch_queries: u32,

    /// External search query rewriting constraints
    pub external_search_max_wiki_terms: u32,
    pub external_search_max_web_terms: u32,
    pub external_search_query_max_chars: u32,

    /// Cross-encoder reranking constraints
    pub rerank_max_candidates: u32,
    pub rerank_query_max_chars: u32,

    /// Corrective retrieval: when the post-rerank sufficiency check fails, the
    /// planner is asked once for different queries and the passes are fused.
    /// The score floor is only read when a cross-encoder actually ran — RRF
    /// ranks are not probabilities and must never be thresholded.
    #[serde(default = "default_sufficiency_min_top_score")]
    pub sufficiency_min_top_score: f32,
    #[serde(default = "default_sufficiency_min_term_coverage")]
    pub sufficiency_min_term_coverage: f32,
    #[serde(default = "default_sufficiency_retry_enabled")]
    pub sufficiency_retry_enabled: bool,

    /// Lexical overlap precision control
    pub overlap_min_hits_for_multi_term: u32,

    /// Document support filter tuning
    pub doc_support_multi_hit_ratio_factor: f32,
    pub doc_support_single_hit_ratio_factor: f32,
    pub doc_support_multi_hit_ratio_min: f32,
    pub doc_support_multi_hit_ratio_max: f32,
    pub doc_support_single_hit_ratio_min: f32,
    pub doc_support_single_hit_ratio_max: f32,
}

fn default_web_fetch_page_count() -> u32 {
    3
}

/// The fetcher's own ceiling, so by default a page is only ever shortened by
/// the turn's page budget — which knows the model's context window — and not
/// by a constant that does not.
fn default_web_page_max_chars() -> u32 {
    50_000
}

fn default_web_page_fetch_timeout_secs() -> u32 {
    12
}

fn default_deep_research_depth() -> u32 {
    3
}

fn default_deep_research_branch_queries() -> u32 {
    3
}

/// `blend_rerank_scores` mixes `0.35 * normalized_first_stage + 0.65 * rerank`,
/// so the first-stage winner is worth 0.35 on its own. A top blended score
/// below that floor means no passage earned any cross-encoder relevance.
fn default_sufficiency_min_top_score() -> f32 {
    0.35
}

/// Fewer than two in five of the planner's informative terms appearing in the
/// top passages means the ranking is confident about text that never mentions
/// the subject. Higher would fire on synonym-heavy corpora, where a passage
/// answers the question without repeating its wording.
fn default_sufficiency_min_term_coverage() -> f32 {
    0.4
}

fn default_sufficiency_retry_enabled() -> bool {
    true
}

impl Default for IndexingSettingsDto {
    fn default() -> Self {
        Self {
            chunk_size: 800,
            chunk_overlap: 120,
            batch_size: 100,
            auto_index_new_files: true,
            file_types: vec![
                "txt".to_string(),
                "md".to_string(),
                "pdf".to_string(),
                "docx".to_string(),
            ],
            indexed_paths: vec![],
            exclude_patterns: vec![
                "*.tmp".to_string(),
                "*.log".to_string(),
                "node_modules".to_string(),
                ".git".to_string(),
            ],
        }
    }
}

impl Default for SearchSettingsDto {
    fn default() -> Self {
        Self {
            max_results: 10,
            similarity_threshold: 0.7,
            enable_reranking: false,
            hybrid_search_alpha: 0.5,
            retrieval_tuning: RetrievalTuningSettingsDto::default(),
            vector_index_compression: VectorIndexCompressionSettingsDto::default(),
            embedding_strategy: EmbeddingStrategySettingDto::default(),
            summary_index_enabled: false,
        }
    }
}

impl Default for VectorIndexCompressionSettingsDto {
    fn default() -> Self {
        Self {
            // Enabled only for models that explicitly advertise Matryoshka
            // support; the composition root falls back to full precision for
            // MiniLM and arbitrary local models. Qwen3 at 256 dims/i8 matched
            // full-precision retrieval in the production-path evaluation.
            mode: VectorIndexCompressionModeDto::Truncated,
            dims: DEFAULT_VECTOR_COMPRESSION_DIMS,
            quantization: VectorQuantizationDto::I8,
        }
    }
}

/// Smallest prefix worth keeping. Below this the Matryoshka objective has no
/// headroom left and the rescore pass is doing all the work.
pub const MIN_VECTOR_COMPRESSION_DIMS: u32 = 32;

/// No embedding model in the catalog is wider than this.
pub const MAX_VECTOR_COMPRESSION_DIMS: u32 = 4096;

/// Evaluated Qwen3 Matryoshka prefix: 16x smaller than 1024-d f32 vectors.
pub const DEFAULT_VECTOR_COMPRESSION_DIMS: u32 = 256;

impl Default for RetrievalTuningSettingsDto {
    fn default() -> Self {
        Self {
            kb_search_min_limit: 16,
            kb_search_max_limit: 48,
            doc_shortlist_candidate_min: 48,
            doc_shortlist_candidate_max: 192,
            doc_shortlist_doc_min: 10,
            doc_shortlist_doc_max: 32,
            shortlist_gate_min_candidates: 8,
            shortlist_gate_min_docs: 4,
            wiki_search_max_results: 5,
            wiki_snippet_max_chars: 1200,
            wiki_context_limit: 3,
            web_search_max_results: 5,
            web_snippet_max_chars: 1200,
            web_fetch_page_count: default_web_fetch_page_count(),
            web_page_max_chars: default_web_page_max_chars(),
            web_page_fetch_timeout_secs: default_web_page_fetch_timeout_secs(),
            deep_research_depth: default_deep_research_depth(),
            deep_research_branch_queries: default_deep_research_branch_queries(),
            external_search_max_wiki_terms: 8,
            external_search_max_web_terms: 10,
            external_search_query_max_chars: 1200,
            rerank_max_candidates: 48,
            rerank_query_max_chars: 6000,
            sufficiency_min_top_score: default_sufficiency_min_top_score(),
            sufficiency_min_term_coverage: default_sufficiency_min_term_coverage(),
            sufficiency_retry_enabled: default_sufficiency_retry_enabled(),
            overlap_min_hits_for_multi_term: 2,
            doc_support_multi_hit_ratio_factor: 0.35,
            doc_support_single_hit_ratio_factor: 0.65,
            doc_support_multi_hit_ratio_min: 0.18,
            doc_support_multi_hit_ratio_max: 0.40,
            doc_support_single_hit_ratio_min: 0.30,
            doc_support_single_hit_ratio_max: 0.55,
        }
    }
}
