//! # Settings Contracts
//!
//! Data Transfer Objects for settings management operations.
//!
//! These DTOs handle requests and responses for application settings management,
//! including retrieval, updates, import/export, and validation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

/// Complete application settings structure.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct SettingsDto {
    /// Indexing configuration
    pub indexing: IndexingSettingsDto,

    /// Search configuration
    pub search: SearchSettingsDto,

    /// LLM configuration
    pub llm: LLMSettingsDto,

    /// UI configuration
    pub ui: UISettingsDto,

    /// Sync configuration
    pub sync: SyncSettingsDto,

    /// Backup configuration
    pub backup: BackupSettingsDto,

    /// Privacy configuration (telemetry, crash reporting)
    #[serde(default)]
    pub privacy: PrivacySettingsDto,

    #[serde(default)]
    pub vault: VaultSettingsDto,

    /// Onboarding state (what the user has already been shown)
    #[serde(default)]
    pub onboarding: OnboardingSettingsDto,
}

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

/// LLM provider selection.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum LLMProvider {
    #[default]
    Auto,
    Local,
    Ollama,
    Llamacpp,
    Openai,
    Anthropic,
}

/// LLM settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LLMSettingsDto {
    /// Provider selection (auto/local/ollama)
    #[serde(default)]
    pub provider: LLMProvider,

    /// Model identifier
    pub model: String,

    /// Temperature for generation (0.0 - 2.0)
    pub temperature: f32,

    /// Top-p (nucleus) sampling
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Top-k sampling
    #[serde(default = "default_top_k")]
    pub top_k: i32,

    /// Repeat penalty (>0.0)
    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// Context window size
    pub context_window: u32,

    /// Ollama server URL
    pub ollama_url: String,

    /// Optional Ollama tag used for utility/router calls.
    /// Empty string means "fall back to `model`".
    #[serde(default)]
    pub ollama_utility_model: String,

    /// Optional Ollama auth header name (e.g., "Authorization")
    #[serde(default)]
    pub ollama_auth_header_name: String,

    /// Optional Ollama auth header value (e.g., "Bearer ...")
    #[serde(default)]
    pub ollama_auth_header_value: String,

    /// Independent remote llama.cpp connection; switching providers retains both.
    #[serde(default)]
    pub llama_cpp: LlamaCppSettingsDto,

    /// Longest silence tolerated inside a response, in seconds.
    ///
    /// The clock resets on every chunk the backend sends, so this bounds how
    /// long a request may produce *nothing* before it is treated as dead. It
    /// does not cap how long an answer may take — total generation time is
    /// governed by the per-turn time budget.
    pub timeout_seconds: u32,

    /// Enable streaming responses
    pub stream_responses: bool,

    /// Prompt templates for chat + RAG
    #[serde(default)]
    pub prompts: LLMPromptSettingsDto,

    /// Verification settings for response grounding indicators.
    #[serde(default)]
    pub verification: LLMVerificationSettingsDto,

    /// Tool output shaping (excerpting, truncation)
    #[serde(default)]
    pub tool_output: ToolOutputSettingsDto,

    /// Router settings for conversation scope and intent routing
    #[serde(default)]
    pub router: RouterSettingsDto,

    /// Additional directories to scan for externally managed local models
    #[serde(default)]
    pub external_model_directories: Vec<String>,

    /// User-defined tool integrations exposed to LLM tool calling.
    #[serde(default)]
    pub custom_tools: Vec<CustomToolSettingsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct LlamaCppSettingsDto {
    pub url: String,
    pub model: String,
    pub auth_header_name: String,
    pub auth_header_value: String,
}

impl Default for LlamaCppSettingsDto {
    fn default() -> Self {
        Self {
            url: "http://localhost:8080".into(),
            model: String::new(),
            auth_header_name: String::new(),
            auth_header_value: String::new(),
        }
    }
}

/// Prompt templates for LLM interactions.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LLMPromptSettingsDto {
    /// Global system prompt injected into every conversation (optional)
    pub system_prompt: String,

    /// Prompt template used for greetings.
    /// Supports placeholders: {question}
    #[serde(default = "default_greeting_prompt_template")]
    pub greeting_prompt_template: String,

    /// Prompt template when context is available.
    /// Supports placeholders: {context}, {question}
    pub rag_prompt_template: String,

    /// Prompt template when no relevant context is found.
    /// Supports placeholders: {question}
    pub no_context_prompt_template: String,

    /// Prompt template used after tool results are appended to context.
    /// Supports placeholders: {question}, {previous_response}
    pub tool_followup_prompt_template: String,
}

/// Verification settings for response grounding checks.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LLMVerificationSettingsDto {
    /// Enable grounding verification and metadata emission for assistant messages.
    pub enabled: bool,
}

/// Tool output shaping settings (excerpts + truncation).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutputSettingsDto {
    /// Maximum characters to include for a tool result in the LLM context
    pub max_chars: u32,

    /// Excerpt length for document/tool content
    pub excerpt_chars: u32,

    /// Maximum search results to summarize from a tool call
    pub max_results: u32,

    /// Maximum highlight terms extracted from the user query
    pub highlight_terms_max: u32,

    /// Per-tool formatting templates
    #[serde(default)]
    pub templates: ToolOutputTemplatesDto,
}

/// Per-tool output templates.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutputTemplatesDto {
    /// Template for generic tool output
    #[serde(default = "default_tool_output_template")]
    pub default_template: String,

    /// Template for get_document output
    #[serde(default = "default_get_document_template")]
    pub get_document_template: String,

    /// Template for semantic_search output
    #[serde(default = "default_semantic_search_template")]
    pub semantic_search_template: String,
}

/// Router settings for conversation scope and routing decisions.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RouterSettingsDto {
    /// Enable LLM-based routing.
    pub enabled: bool,

    /// Router model identifier (required).
    pub model: String,

    /// Router timeout in milliseconds.
    pub timeout_ms: u64,

    /// Max tokens for router responses.
    pub max_tokens: u32,

    /// Router temperature (lower = more deterministic).
    pub temperature: f32,

    /// Ambiguity threshold (0-1). If confidence is below this, ask for clarification.
    pub ambiguity_threshold: f32,

    /// Prefer the last referenced document when ambiguous.
    pub prefer_last_document: bool,

    /// Router prompt template (JSON output expected).
    pub prompt_template: String,

    /// Clarification prompt template shown to users.
    pub clarify_prompt_template: String,
}

/// User-defined tool configuration.
///
/// These tools are exposed to the chat model as callable functions and executed
/// through a controlled HTTP GET workflow.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolSettingsDto {
    /// Enables or disables this custom tool.
    #[serde(default = "default_custom_tool_enabled")]
    pub enabled: bool,

    /// Function name exposed to the LLM (alphanumeric + underscores).
    pub name: String,

    /// Human-readable description shown in tool metadata.
    pub description: String,

    /// Base endpoint URL for this tool.
    pub endpoint: String,

    /// Query string parameter name used for user query text.
    #[serde(default = "default_custom_tool_query_param")]
    pub query_param: String,

    /// Optional parameter name used to pass max results.
    #[serde(default = "default_custom_tool_max_results_param")]
    pub max_results_param: Option<String>,

    /// Default max results when the model omits it.
    #[serde(default = "default_custom_tool_default_max_results")]
    pub default_max_results: u32,
}

/// UI settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UISettingsDto {
    /// Theme (light/dark/system)
    pub theme: String,

    /// Font size in pixels
    pub font_size: u32,

    /// Show preview pane
    pub show_preview: bool,

    /// Results per page
    pub results_per_page: u32,

    /// Enable animations
    pub enable_animations: bool,
}

/// Sync settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettingsDto {
    /// Sync feature enabled
    pub sync_enabled: bool,

    /// Sync server URL
    pub sync_url: String,

    /// Sync interval in minutes
    pub sync_interval_minutes: u32,

    /// Automatically sync on changes
    pub auto_sync: bool,

    /// Sync on application startup
    pub sync_on_startup: bool,
}

/// Backup settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettingsDto {
    /// Auto-backup enabled
    pub auto_backup_enabled: bool,

    /// Backup frequency (daily/weekly/monthly)
    pub backup_frequency: String,

    /// Number of days to retain backups
    pub backup_retention_days: u32,

    /// Path to backup directory
    pub backup_path: String,

    /// Compress backups
    pub compress_backups: bool,
}

/// Privacy settings — defaults to opt-out for both flags.
///
/// These flags must be read by the backend before emitting telemetry or
/// crash reports. Source of truth lives here, not in the frontend
/// localStorage before moving to backend-owned settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct PrivacySettingsDto {
    /// Send anonymous usage statistics. Off by default.
    pub telemetry_enabled: bool,

    /// Send crash reports. Off by default.
    pub crash_reporting: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct VaultSettingsDto {
    /// Empty string = `~/Lattice`.
    #[serde(default)]
    pub vault_path: String,

    #[serde(default)]
    pub enabled: bool,

    /// Reverse-direction sync: import external `.md` edits back into SQL.
    #[serde(default)]
    pub watch_external_changes: bool,
}

/// Onboarding state — what the user has already been through.
///
/// Persisted first-run state, and the single source of truth for the
/// first-run gate.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct OnboardingSettingsDto {
    /// True once the user has installed the recommended models, chosen their
    /// own, or said "not now". The first-run modal never re-opens while set.
    #[serde(default)]
    pub first_run_dismissed: bool,
}

/// Settings category identifier.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum SettingsCategory {
    /// Indexing settings
    Indexing,
    /// Search settings
    Search,
    /// LLM settings
    Llm,
    /// UI settings
    Ui,
    /// Sync settings
    Sync,
    /// Backup settings
    Backup,
    /// Privacy settings (telemetry + crash reporting)
    Privacy,
    /// Vault portability (markdown mirror folder)
    Vault,
    /// Onboarding state (first-run gate)
    Onboarding,
}

impl SettingsCategory {
    /// Get all categories.
    pub fn all() -> Vec<SettingsCategory> {
        vec![
            SettingsCategory::Indexing,
            SettingsCategory::Search,
            SettingsCategory::Llm,
            SettingsCategory::Ui,
            SettingsCategory::Sync,
            SettingsCategory::Backup,
            SettingsCategory::Privacy,
            SettingsCategory::Vault,
            SettingsCategory::Onboarding,
        ]
    }

    /// Convert to string key.
    pub fn as_str(&self) -> &'static str {
        match self {
            SettingsCategory::Indexing => "indexing",
            SettingsCategory::Search => "search",
            SettingsCategory::Llm => "llm",
            SettingsCategory::Ui => "ui",
            SettingsCategory::Sync => "sync",
            SettingsCategory::Backup => "backup",
            SettingsCategory::Privacy => "privacy",
            SettingsCategory::Vault => "vault",
            SettingsCategory::Onboarding => "onboarding",
        }
    }
}

impl FromStr for SettingsCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "indexing" => Ok(SettingsCategory::Indexing),
            "search" => Ok(SettingsCategory::Search),
            "llm" => Ok(SettingsCategory::Llm),
            "ui" => Ok(SettingsCategory::Ui),
            "sync" => Ok(SettingsCategory::Sync),
            "backup" => Ok(SettingsCategory::Backup),
            "privacy" => Ok(SettingsCategory::Privacy),
            "vault" => Ok(SettingsCategory::Vault),
            "onboarding" => Ok(SettingsCategory::Onboarding),
            _ => Err(format!("Unknown settings category: {}", s)),
        }
    }
}

/// Request to update settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequestDto {
    /// Category to update (optional - updates all if not specified)
    pub category: Option<SettingsCategory>,

    /// Settings updates as key-value pairs
    pub updates: HashMap<String, serde_json::Value>,
}

/// Request to reset settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResetSettingsRequestDto {
    /// Category to reset (optional - resets all if not specified)
    pub category: Option<SettingsCategory>,
}

/// Request to export settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettingsRequestDto {
    /// Path where settings should be exported
    pub path: String,
}

/// Response from export operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettingsResponseDto {
    /// Path where settings were exported
    pub path: PathBuf,

    /// Status message
    pub status: String,
}

/// Request to import settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSettingsRequestDto {
    /// Path to settings file
    pub path: String,

    /// Merge with existing settings instead of replacing
    pub merge: bool,
}

/// Response from import operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSettingsResponseDto {
    /// Updated settings
    pub settings: SettingsDto,

    /// Status message
    pub status: String,
}

/// Result of settings validation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    /// Overall validation success
    pub valid: bool,

    /// Validation errors by category
    pub errors: HashMap<String, Vec<String>>,

    /// Validation warnings
    pub warnings: HashMap<String, Vec<String>>,
}

impl ValidationResult {
    /// Create a successful validation result.
    pub fn success() -> Self {
        Self {
            valid: true,
            errors: HashMap::new(),
            warnings: HashMap::new(),
        }
    }

    /// Create a failed validation result.
    pub fn failure() -> Self {
        Self {
            valid: false,
            errors: HashMap::new(),
            warnings: HashMap::new(),
        }
    }

    /// Add an error to a category.
    pub fn add_error(&mut self, category: &str, error: String) {
        self.valid = false;
        self.errors
            .entry(category.to_string())
            .or_default()
            .push(error);
    }

    /// Add a warning to a category.
    pub fn add_warning(&mut self, category: &str, warning: String) {
        self.warnings
            .entry(category.to_string())
            .or_default()
            .push(warning);
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

fn default_top_p() -> f32 {
    0.9
}

fn default_top_k() -> i32 {
    40
}

fn default_repeat_penalty() -> f32 {
    1.1
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

fn default_custom_tool_enabled() -> bool {
    true
}

fn default_custom_tool_query_param() -> String {
    "q".to_string()
}

fn default_custom_tool_max_results_param() -> Option<String> {
    Some("limit".to_string())
}

fn default_custom_tool_default_max_results() -> u32 {
    5
}

fn default_greeting_prompt_template() -> String {
    "The user greeted you: \"{question}\". Reply briefly and warmly, then offer help with documents, web search, or general questions."
        .to_string()
}

fn default_tool_output_template() -> String {
    "Tool output:\n{json}".to_string()
}

fn default_get_document_template() -> String {
    "Document ID: {document_id}\nTitle: {title}\nChunks: {chunk_count}\nExcerpt: {excerpt}{truncated}"
        .to_string()
}

fn default_semantic_search_template() -> String {
    "Search results (showing {shown} of {total_found}):\n{results}".to_string()
}

impl Default for RouterSettingsDto {
    fn default() -> Self {
        Self {
            enabled: false,
            model: "phi-3.5-mini-instruct-q4_k_m".to_string(),
            timeout_ms: 350,
            max_tokens: 120,
            temperature: 0.1,
            ambiguity_threshold: 0.62,
            prefer_last_document: true,
            prompt_template: "You are a routing assistant for a knowledge base chat.\nReturn ONLY valid JSON with the fields:\n- action: \"use_last_document\" | \"new_search\" | \"clarify\"\n- confidence: number from 0 to 1\n- clarify_question: string or null\n- rationale: short string\n\nUser query: {query}\nHas recent document: {has_recent_document}\nRecent document title: {recent_document_title}\nRecent document id: {recent_document_id}\n\nDecide whether the user is referring to the recent document or wants a new search. If uncertain, choose \"clarify\".\nJSON:".to_string(),
            clarify_prompt_template: "Are you asking about the recent document ({recent_document_title}), or should I search for something else?".to_string(),
        }
    }
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

/// Shortest silence worth calling a stall. A backend that is ingesting a large
/// prompt, or loading weights on a cold cache, legitimately emits nothing for
/// several seconds; below this the retry path kills healthy requests and can
/// livelock on the re-ingest of the same prompt.
pub const MIN_LLM_STALL_TIMEOUT_SECONDS: u32 = 15;

/// Longest silence worth waiting through. Past this a hung backend is
/// indistinguishable from a slow one, which is what stall detection is for.
pub const MAX_LLM_STALL_TIMEOUT_SECONDS: u32 = 180;

/// Long enough to cover prompt ingestion on a local model, short enough that a
/// dead backend surfaces while the user is still looking at the answer.
pub const DEFAULT_LLM_STALL_TIMEOUT_SECONDS: u32 = 30;

impl Default for LLMSettingsDto {
    fn default() -> Self {
        Self {
            provider: LLMProvider::Auto,
            model: "llama3.2:latest".to_string(),
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            repeat_penalty: 1.1,
            max_tokens: 131072,
            context_window: 131072,
            ollama_url: "http://localhost:11434".to_string(),
            ollama_utility_model: String::new(),
            ollama_auth_header_name: String::new(),
            ollama_auth_header_value: String::new(),
            llama_cpp: LlamaCppSettingsDto::default(),
            timeout_seconds: DEFAULT_LLM_STALL_TIMEOUT_SECONDS,
            stream_responses: true,
            prompts: LLMPromptSettingsDto::default(),
            verification: LLMVerificationSettingsDto::default(),
            tool_output: ToolOutputSettingsDto::default(),
            router: RouterSettingsDto::default(),
            external_model_directories: Vec::new(),
            custom_tools: Vec::new(),
        }
    }
}

impl Default for LLMPromptSettingsDto {
    fn default() -> Self {
        Self {
            system_prompt: "You are Lattice, a precise research assistant. Use the user's documents when available. Cite sources using numeric brackets like [1], [2], [3]. Never fabricate document IDs.".to_string(),
            greeting_prompt_template: default_greeting_prompt_template(),
            rag_prompt_template: "Answer the user's question using only the provided context. Cite every factual statement supported by the context using numeric brackets like [1], [2], [3]. If the context does not contain the answer, say that the answer is not available in the provided documents and do not guess. When the answer is unavailable, respond concisely without summarizing or citing unrelated context. Do not cite a source that does not support the associated statement. If you need to call get_document, use the exact Document ID shown in the context. For long documents, request additional pages with the page parameter.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:".to_string(),
            no_context_prompt_template: "The user asked: \"{question}\"\n\nNo relevant documents were found in local documents for this turn. Respond helpfully using general knowledge when appropriate, and suggest web search or adding documents if they want sourced evidence.".to_string(),
            tool_followup_prompt_template: "Tool results have been added to the context. Use them to answer the user's question. If excerpts are provided, quote them briefly and avoid repetition.\n\nQuestion: {question}\n{previous_response}\nAnswer:".to_string(),
        }
    }
}

impl Default for LLMVerificationSettingsDto {
    fn default() -> Self {
        // Verification annotates the completed answer; it never rewrites or
        // suppresses it. Keep the safety signal on and let users who prefer
        // lower post-generation latency opt out.
        Self { enabled: true }
    }
}

impl Default for ToolOutputSettingsDto {
    fn default() -> Self {
        Self {
            max_chars: 50000,
            excerpt_chars: 4000,
            max_results: 12,
            highlight_terms_max: 8,
            templates: ToolOutputTemplatesDto::default(),
        }
    }
}

impl Default for ToolOutputTemplatesDto {
    fn default() -> Self {
        Self {
            default_template: default_tool_output_template(),
            get_document_template: default_get_document_template(),
            semantic_search_template: default_semantic_search_template(),
        }
    }
}

impl Default for UISettingsDto {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            font_size: 14,
            show_preview: true,
            results_per_page: 10,
            enable_animations: true,
        }
    }
}

impl Default for SyncSettingsDto {
    fn default() -> Self {
        Self {
            sync_enabled: false,
            sync_url: String::new(),
            sync_interval_minutes: 30,
            auto_sync: false,
            sync_on_startup: false,
        }
    }
}

impl Default for BackupSettingsDto {
    fn default() -> Self {
        Self {
            auto_backup_enabled: false,
            backup_frequency: "daily".to_string(),
            backup_retention_days: 30,
            backup_path: String::new(),
            compress_backups: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_dto_default() {
        let settings = SettingsDto::default();

        assert_eq!(settings.indexing.chunk_size, 800);
        assert_eq!(settings.search.max_results, 10);
        assert_eq!(
            settings.search.vector_index_compression.mode,
            VectorIndexCompressionModeDto::Truncated
        );
        assert_eq!(
            settings.search.vector_index_compression.dims,
            DEFAULT_VECTOR_COMPRESSION_DIMS
        );
        assert_eq!(settings.llm.model, "llama3.2:latest");
        assert!(settings.llm.verification.enabled);
        assert_eq!(settings.ui.theme, "system");
    }

    #[test]
    fn test_settings_dto_serialization() {
        let settings = SettingsDto::default();
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: SettingsDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.indexing.chunk_size, 800);
    }

    #[test]
    fn test_settings_category_from_str() {
        assert_eq!("indexing".parse(), Ok(SettingsCategory::Indexing));
        assert_eq!("search".parse(), Ok(SettingsCategory::Search));
        assert!("invalid".parse::<SettingsCategory>().is_err());
    }

    #[test]
    fn test_settings_category_as_str() {
        assert_eq!(SettingsCategory::Indexing.as_str(), "indexing");
        assert_eq!(SettingsCategory::Search.as_str(), "search");
    }

    #[test]
    fn test_validation_result_success() {
        let result = ValidationResult::success();
        assert!(result.valid);
        assert!(!result.has_errors());
        assert!(!result.has_warnings());
    }

    #[test]
    fn test_validation_result_add_error() {
        let mut result = ValidationResult::success();
        result.add_error("indexing", "Invalid chunk size".to_string());

        assert!(!result.valid);
        assert!(result.has_errors());
        assert_eq!(result.errors.get("indexing").unwrap().len(), 1);
    }

    #[test]
    fn test_validation_result_add_warning() {
        let mut result = ValidationResult::success();
        result.add_warning("llm", "High temperature value".to_string());

        assert!(result.valid);
        assert!(result.has_warnings());
        assert_eq!(result.warnings.get("llm").unwrap().len(), 1);
    }
}
