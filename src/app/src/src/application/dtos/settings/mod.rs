//! # Settings DTOs
//!
//! Data Transfer Objects for settings management operations.
//!
//! These DTOs handle requests and responses for application settings management,
//! including retrieval, updates, import/export, and validation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

// ============================================================================
// Settings Structure DTOs
// ============================================================================

/// Complete application settings structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// Indexing settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Paths to exclude from indexing
    pub excluded_paths: Vec<String>,
}

/// Search settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// Retrieval pipeline tuning settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LLMProvider {
    #[default]
    Auto,
    Local,
    Ollama,
}

/// LLM settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Optional Ollama auth header name (e.g., "Authorization")
    #[serde(default)]
    pub ollama_auth_header_name: String,

    /// Optional Ollama auth header value (e.g., "Bearer ...")
    #[serde(default)]
    pub ollama_auth_header_value: String,

    /// Request timeout in seconds
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

/// Prompt templates for LLM interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMVerificationSettingsDto {
    /// Enable grounding verification and metadata emission for assistant messages.
    pub enabled: bool,
}

/// Tool output shaping settings (excerpts + truncation).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ============================================================================
// Settings Category Enum
// ============================================================================

/// Settings category identifier.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
            _ => Err(format!("Unknown settings category: {}", s)),
        }
    }
}

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Request to update settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequestDto {
    /// Category to update (optional - updates all if not specified)
    pub category: Option<SettingsCategory>,

    /// Settings updates as key-value pairs
    pub updates: HashMap<String, serde_json::Value>,
}

/// Request to reset settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetSettingsRequestDto {
    /// Category to reset (optional - resets all if not specified)
    pub category: Option<SettingsCategory>,
}

/// Request to export settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettingsRequestDto {
    /// Path where settings should be exported
    pub path: String,
}

/// Response from export operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettingsResponseDto {
    /// Path where settings were exported
    pub path: PathBuf,

    /// Status message
    pub status: String,
}

/// Request to import settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSettingsRequestDto {
    /// Path to settings file
    pub path: String,

    /// Merge with existing settings instead of replacing
    pub merge: bool,
}

/// Response from import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSettingsResponseDto {
    /// Updated settings
    pub settings: SettingsDto,

    /// Status message
    pub status: String,
}

// ============================================================================
// Validation DTOs
// ============================================================================

/// Result of settings validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ============================================================================
// Default Implementations
// ============================================================================

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
            // Default to a small, fast local model suitable for routing.
            // Users can override this in Settings → Model Catalog.
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
            excluded_paths: vec![],
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
        }
    }
}

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
            ollama_auth_header_name: String::new(),
            ollama_auth_header_value: String::new(),
            timeout_seconds: 30,
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
            system_prompt: "You are Recall, a precise research assistant. Use the user's documents when available. Cite sources using numeric brackets like [1], [2], [3]. Never fabricate document IDs.".to_string(),
            greeting_prompt_template: default_greeting_prompt_template(),
            rag_prompt_template: "Answer the user's question using only the provided context. Cite sources using numeric brackets like [1], [2], [3]. If you need to call get_document, use the exact Document ID shown in the context. For long documents, request additional pages with the page parameter.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:".to_string(),
            no_context_prompt_template: "The user asked: \"{question}\"\n\nNo relevant documents were found in local documents for this turn. Respond helpfully using general knowledge when appropriate, and suggest web search or adding documents if they want sourced evidence.".to_string(),
            tool_followup_prompt_template: "Tool results have been added to the context. Use them to answer the user's question. If excerpts are provided, quote them briefly and avoid repetition.\n\nQuestion: {question}\n{previous_response}\nAnswer:".to_string(),
        }
    }
}

impl Default for LLMVerificationSettingsDto {
    fn default() -> Self {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_dto_default() {
        let settings = SettingsDto::default();

        assert_eq!(settings.indexing.chunk_size, 800);
        assert_eq!(settings.search.max_results, 10);
        assert_eq!(settings.llm.model, "llama3.2:latest");
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
