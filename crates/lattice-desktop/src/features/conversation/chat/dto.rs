use super::retrieval::WEB_SOURCE_PREFIX;
use super::{
    CapturedSourcesDto, RetrievalSubTimingMetrics, ToolLoopTimingMetrics, TurnModelDto,
    TurnStepDto, VerificationReadyDto,
};
use lattice_core::application::contracts::explorer::ExplorerFocusDto;
pub(super) use lattice_core::application::contracts::sources::SourceDto;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// Chat completion and conversation reload use the same serialized message contract.
pub type ConversationMessage = crate::features::conversation::dto::MessageDto;

/// Chat response with conversation metadata
#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    /// ID of the conversation (use for subsequent messages)
    pub conversation_id: String,
    /// LLM's response message (DEPRECATED: use messages instead)
    pub message: String,
    /// All messages from database (NEW: Real source of truth)
    pub messages: Vec<ConversationMessage>,
    /// Number of context messages used
    pub context_used: usize,
    /// Source documents used for RAG context (for citation display)
    pub sources: Vec<SourceDto>,
    /// Detailed per-layer timing metrics in milliseconds
    pub timing_metrics: Option<ConversationFlowTimingMetrics>,
}

/// What a turn's sources amount to, for the line the UI shows above an answer.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct TraceCounts {
    /// Passages from documents in the user's vault.
    pub(super) passages: usize,
    /// Distinct vault documents those passages came from.
    pub(super) files: usize,
    /// Distinct web pages, which are not files and are never counted as any.
    pub(super) web_pages: usize,
}

/// Splits a turn's sources into vault documents and web pages by their id
/// prefix, so web results are never counted as the user's files.
pub(super) fn trace_counts(sources: &[SourceDto]) -> TraceCounts {
    let (vault, web): (Vec<_>, Vec<_>) = sources
        .iter()
        .partition(|source| !source.document_id.starts_with(WEB_SOURCE_PREFIX));
    TraceCounts {
        passages: vault
            .iter()
            .map(|source| {
                source
                    .chunk_excerpts
                    .as_ref()
                    .map_or(1, |excerpts| excerpts.len().max(1))
            })
            .sum(),
        files: vault
            .iter()
            .map(|source| source.document_id.as_str())
            .collect::<HashSet<_>>()
            .len(),
        web_pages: web
            .iter()
            .map(|source| source.document_id.as_str())
            .collect::<HashSet<_>>()
            .len(),
    }
}

/// Keeps a count of zero out of the wire entirely, so a trace that touched no
/// web pages looks exactly like one written before the field existed.
fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// Retrieval trace for one turn, shared by streaming events, persisted message
/// metadata and generated bindings. `searched_documents` is the size of the
/// scoped document set, not the corpus.
#[derive(Debug, Clone, Serialize, Deserialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalTraceDto {
    pub searched_documents: usize,
    pub passages: usize,
    pub files: usize,

    /// Web pages carried into the answer, counted apart from `files`, which means
    /// vault documents only.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub web_pages: usize,
    /// "vault" | "linked"
    pub scope: String,
    /// Why the knowledge base could not be searched, when it could not be.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,

    /// The post-rerank sufficiency verdict, after any corrective pass. Absent on
    /// traces saved before the check existed; absent is not insufficient.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kb_sufficient: Option<bool>,

    /// How many replanned retrieval passes ran. Capped at one per turn, so this
    /// is 0 or 1; absent when the check did not run at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kb_corrective_retries: Option<u32>,

    /// True when a short follow-up reused the previous turn's topic and the
    /// planner LLM call was skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kb_planner_skipped: Option<bool>,

    /// Why the verdict went the way it did — stable codes (`low_top_score`,
    /// `low_term_coverage`, …), not prose, so the UI and tests can match on
    /// them.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sufficiency_reasons: Vec<String>,

    /// How many documents this turn was pinned to, when it was pinned at all.
    ///
    /// The count after the intersection with the space scope, so `Some(0)`
    /// means the request named documents this chat cannot reach and the turn
    /// searched nothing — which is what fails closed looks like from outside.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_documents: Option<usize>,
}

/// Every chat stream update identifies its conversation and generation. Other
/// producers share the event channel, so consumers must require both IDs.
#[derive(Debug, Clone, Serialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamEventDto {
    pub conversation_id: String,
    pub request_id: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval: Option<RetrievalTraceDto>,

    /// One step of the turn, starting or finishing. A tool round streams no text,
    /// so this is what the UI shows meanwhile; the finished answer's timeline is
    /// this same list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<TurnStepDto>,

    /// A grounding check that finished after the turn returned. Arrives once
    /// per verified answer, possibly well after `done`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationReadyDto>,

    /// The model this turn is pinned to. Sent once, as soon as the turn has
    /// it, so the reader knows what is answering before the first token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<TurnModelDto>,

    /// Cited pages captured after the turn returned. Arrives once per answer
    /// that cited a page it never read, before that answer's grounding check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured: Option<CapturedSourcesDto>,

    /// Pictures a `find_image` call just stored, for the answer's `img-N`
    /// lines to resolve against while it streams. Merged by `ref`; the saved
    /// message carries the attached ones in `metadata.images`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<lattice_core::application::contracts::images::ChatImageDto>>,
}

impl ChatStreamEventDto {
    pub(in crate::features::conversation) fn new(conversation_id: &str, request_id: &str) -> Self {
        Self {
            conversation_id: conversation_id.to_owned(),
            request_id: request_id.to_owned(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolPreferences {
    #[serde(default)]
    pub knowledge_base: bool,
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub deep_research_mode: bool,
    #[serde(default)]
    pub followup_mode: bool,
    #[serde(default)]
    pub turn_mode: Option<String>,
    #[serde(default)]
    pub enabled_tools: Option<Vec<String>>,
    /// Documents this chat is pinned to; empty or absent means the whole space. A
    /// request may only narrow what the turn reads, so these are intersected with
    /// the space scope (see `focus_scope` in the retrieval pipeline).
    #[serde(default)]
    pub focus_document_ids: Option<Vec<String>>,
    /// The file open in Explorer and its selected lines, sent with each turn
    /// of an Explorer conversation. Resolved against the conversation's stored
    /// folder; ignored when it has none, and dropped when it does not resolve.
    #[serde(default)]
    pub explorer_focus: Option<ExplorerFocusDto>,
    /// The message already carries everything the turn may use: no retrieval,
    /// tools or source verification. Backend callers only; never deserialized. Not
    /// the same as `knowledge_base: false`, which only declines to force a vault
    /// search while the router may still run one.
    #[serde(skip)]
    pub closed_book: bool,
}

#[derive(Debug, Serialize, Clone, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationFlowTimingMetrics {
    pub validate_request_ms: u64,
    pub load_llm_ms: u64,
    pub conversation_init_ms: u64,
    pub settings_load_ms: u64,
    pub context_build_ms: u64,
    pub router_ms: u64,
    pub retrieval_pipeline_ms: u64,
    pub retrieval_subtimings: Option<RetrievalSubTimingMetrics>,
    pub prompt_build_ms: u64,
    pub persist_user_message_ms: u64,
    pub tool_prep_ms: u64,
    pub generation_ms: u64,
    pub generation_subtimings: Option<ToolLoopTimingMetrics>,
    pub verification_ms: u64,
    pub finalize_persistence_ms: u64,
    pub total_ms: u64,
}
