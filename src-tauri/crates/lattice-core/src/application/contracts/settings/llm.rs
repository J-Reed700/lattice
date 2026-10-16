//! Model, prompt, verification, tool-output, router and custom-tool settings and their defaults.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::models::model_ref::ModelRef;

use super::generation::{
    LocalModelConfigDto, SamplingProfileDto, SpeculativeDecodingDto, ThinkingOverrideDto,
};

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
    Google,
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

    /// Flat penalty on any token already in the recent window. The Qwen 3.5
    /// cards recommend 1.5 in both modes to stop loops, and warn it can
    /// occasionally mix languages, so it stays the user's to lower. Zero is off.
    #[serde(default = "default_presence_penalty")]
    pub presence_penalty: f32,

    /// Let the model think before an answer that offers it no tools. Rounds
    /// that can call tools never think: Qwen 3.5 9B writes its calls inside
    /// the thinking block, where llama.cpp's tool grammar ends generation.
    #[serde(default)]
    pub answer_thinking: bool,

    /// Pin thinking on or off for every request, overriding the rule above
    /// and the model's own (Laguna always thinks: with thinking off it
    /// repeats one special token). `Auto` keeps both rules.
    #[serde(default)]
    pub thinking_override: ThinkingOverrideDto,

    /// Sampling for requests that think before answering. The fields above
    /// are the profile for answering directly.
    #[serde(default)]
    pub thinking_sampling: SamplingProfileDto,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// Context window size
    pub context_window: u32,

    /// Context window for the bundled local model, in tokens.
    ///
    /// `None` means Auto: the window is sized from what the GGUF says the model
    /// was trained for and how much memory the accelerator reports, which is
    /// almost always the right answer. A number here overrides that and is used
    /// as written — llama.cpp will rope-scale past the trained length, and an
    /// allocation too large for the card falls back rather than being silently
    /// shrunk. `context_window` above governs remote providers, not this.
    #[serde(default)]
    pub local_context_window: Option<u32>,

    /// Per-model launch overrides keyed by `model_id` (context length,
    /// concurrent predictions, seed, KV cache quantization).
    #[serde(default)]
    pub local_model_configs: BTreeMap<String, LocalModelConfigDto>,

    /// Speculative decoding for the bundled local server. Experimental, and
    /// applied only to a model that carries a multi-token-prediction head;
    /// every other model starts as it always has.
    #[serde(default)]
    pub speculative_decoding: SpeculativeDecodingDto,

    /// Ollama server URL
    pub ollama_url: String,

    /// Whether the user has saved or selected an Ollama connection.
    /// Default URL/model suggestions alone do not constitute a connection.
    #[serde(default)]
    pub ollama_configured: bool,

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

    /// The model every learning job runs on: lesson preparation, outlines,
    /// practice, study decks, assessments, diagnostics, learning memory.
    /// `None` means not set, and learning work refuses until it is; it never
    /// borrows the chat default.
    #[serde(default)]
    pub learning_model: Option<ModelRef>,

    /// Additional directories to scan for externally managed local models
    #[serde(default)]
    pub external_model_directories: Vec<String>,

    /// User-defined tool integrations exposed to LLM tool calling.
    #[serde(default)]
    pub custom_tools: Vec<CustomToolSettingsDto>,

    /// Bounded, source-backed memory. Enabled for new settings and missing fields.
    /// An explicitly saved false remains an opt-out. Model extraction quality is
    /// evaluated separately from deterministic provenance and budget guarantees.
    #[serde(default = "default_bounded_memory")]
    pub bounded_conversation_memory: bool,
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
}

/// Verification settings for response grounding checks.
///
/// The judge's sampling lives here rather than being inherited from
/// [`LLMSettingsDto`]: a verdict is a classification, not a composition, and
/// running it at the chat model's creative temperature makes the same claim
/// against the same passage come out differently from one turn to the next.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LLMVerificationSettingsDto {
    /// Enable grounding verification and metadata emission for assistant messages.
    pub enabled: bool,

    /// Sampling temperature for the claim judge. Zero is greedy decoding, which
    /// is what a verdict wants: the same evidence must produce the same answer.
    #[serde(default = "default_verification_temperature")]
    pub temperature: f32,

    /// Nucleus sampling for the claim judge. One disables it, leaving the
    /// temperature as the only knob that can introduce variance.
    #[serde(default = "default_verification_top_p")]
    pub top_p: f32,

    /// Top-k sampling for the claim judge. One takes the argmax token.
    #[serde(default = "default_verification_top_k")]
    pub top_k: i32,
}

fn default_verification_temperature() -> f32 {
    0.0
}

fn default_verification_top_p() -> f32 {
    1.0
}

fn default_verification_top_k() -> i32 {
    1
}

/// Tool output shaping settings (excerpts + truncation).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutputSettingsDto {
    /// Length of the excerpt a source card shows for a document or tool
    /// result. Display only: what a tool result carries into the model's
    /// context is decided by the window (see the tool loop's allowance).
    pub excerpt_chars: u32,

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

// The answer defaults are the Qwen 3.5 card's non-thinking profile, the
// family the curated catalog leads with.
fn default_top_p() -> f32 {
    0.8
}

fn default_top_k() -> i32 {
    20
}

fn default_repeat_penalty() -> f32 {
    1.0
}

fn default_presence_penalty() -> f32 {
    1.5
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
            model: "qwen3.5-4b-q4_k_m".to_string(),
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
            top_p: default_top_p(),
            top_k: default_top_k(),
            repeat_penalty: default_repeat_penalty(),
            presence_penalty: default_presence_penalty(),
            answer_thinking: false,
            thinking_override: ThinkingOverrideDto::Auto,
            thinking_sampling: SamplingProfileDto::thinking(),
            max_tokens: 131072,
            context_window: 131072,
            local_context_window: None,
            local_model_configs: BTreeMap::new(),
            speculative_decoding: SpeculativeDecodingDto::Off,
            ollama_url: "http://localhost:11434".to_string(),
            ollama_configured: false,
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
            learning_model: None,
            external_model_directories: Vec::new(),
            custom_tools: Vec::new(),
            bounded_conversation_memory: true,
        }
    }
}

impl Default for LLMPromptSettingsDto {
    fn default() -> Self {
        Self {
            system_prompt: "You are Lattice, a precise research assistant. Use the user's documents when available. Cite sources using numeric brackets like [1], [2], [3]. Never fabricate document IDs.".to_string(),
            greeting_prompt_template: default_greeting_prompt_template(),
            rag_prompt_template: "Answer the user's question using only the provided context. Cite every factual statement supported by the context using numeric brackets like [1], [2], [3]. If the context does not contain the answer, say that the answer is not available in the provided documents and do not guess. When the answer is unavailable, respond concisely without summarizing or citing unrelated context. Do not cite a source that does not support the associated statement. If you need to call get_document, use the exact Document ID shown in the context. For long documents, read on with the offset parameter each result names.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:".to_string(),
            no_context_prompt_template: "The user asked: \"{question}\"\n\n{context}\n\nAnswer from general knowledge where you can, and say plainly that this answer is not backed by their own documents. If they want sourced evidence, offer a web search or adding documents to their lattice.".to_string(),
        }
    }
}

impl Default for LLMVerificationSettingsDto {
    fn default() -> Self {
        // Verification annotates the completed answer; it never rewrites or
        // suppresses it. Keep the safety signal on and let users who prefer
        // lower post-generation latency opt out.
        Self {
            enabled: true,
            temperature: default_verification_temperature(),
            top_p: default_verification_top_p(),
            top_k: default_verification_top_k(),
        }
    }
}

impl Default for ToolOutputSettingsDto {
    fn default() -> Self {
        Self {
            excerpt_chars: 4000,
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

fn default_bounded_memory() -> bool {
    true
}
