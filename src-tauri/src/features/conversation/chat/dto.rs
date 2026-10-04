use super::retrieval::WEB_SOURCE_PREFIX;
use super::{RetrievalSubTimingMetrics, ToolLoopTimingMetrics, TurnStepDto, VerificationReadyDto};
use crate::features::qa::dto::SourceDto;
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

/// Split a turn's sources into vault documents and web pages.
///
/// Web results are shaped like document sources so citations can treat them
/// alike, and the counts used to be taken over the whole list. A web-only
/// answer in a space containing no documents therefore announced "10 passages
/// from 10 files" — a claim to have read ten of the user's documents, made by a
/// turn that never touched the vault. The prefix on the id is what separates
/// them.
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

/// Retrieval trace for one turn.
///
/// `searched_documents` is the size of the document set the hard space-scope
/// filter actually allowed, not the size of the corpus — a scoped conversation
/// must not claim to have read the whole vault.
///
/// Shared by streaming events, persisted message metadata, and generated bindings.
#[derive(Debug, Clone, Serialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalTraceDto {
    pub searched_documents: usize,
    pub passages: usize,
    pub files: usize,

    /// Web pages carried into the answer, counted apart from `files`.
    ///
    /// `files` means documents in the user's vault and nothing else. Web
    /// results used to be counted there too, so a web-only answer in a space
    /// holding no documents still reported "10 passages from 10 files" — which
    /// reads as though it had searched the vault, and is the sort of claim that
    /// makes a correctly scoped answer look like a leak.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub web_pages: usize,
    /// "vault" | "linked"
    pub scope: String,
    /// Why the knowledge base could not be searched, when it could not be.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,

    /// The post-rerank sufficiency verdict, after any corrective pass.
    ///
    /// Additive and absent by default: traces persisted before the sufficiency
    /// check existed carry no verdict, and absent is not the same as
    /// insufficient. Every consumer must tolerate `null` rather than read a
    /// missing verdict as a failure.
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

    /// One step of the turn, starting or finishing.
    ///
    /// A tool round emits no text at all while the model reasons and writes its
    /// tool calls — on a slow model that is minutes of a spinner with nothing
    /// behind it, which is indistinguishable from a hang. This is what the UI
    /// has during that stretch, and unlike the sentence it replaces it is kept:
    /// the timeline under the finished answer is this same list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<TurnStepDto>,

    /// A grounding check that finished after the turn returned. Arrives once
    /// per verified answer, possibly well after `done`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationReadyDto>,
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

#[derive(Debug, Clone, Default, Deserialize, specta::Type)]
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
    /// Documents this chat is pinned to. Empty or absent means the whole space.
    ///
    /// A request may only ever narrow what the turn can read, so these ids are
    /// intersected with the conversation's space scope before anything uses
    /// them; ids from outside it are dropped. See `focus_scope` in the
    /// retrieval pipeline.
    #[serde(default)]
    pub focus_document_ids: Option<Vec<String>>,
    /// The file open in Explorer and its selected lines, sent with each turn
    /// of an Explorer conversation. Resolved against the conversation's stored
    /// folder; ignored when it has none, and dropped when it does not resolve.
    #[serde(default)]
    pub explorer_focus: Option<crate::features::explorer::dto::ExplorerFocusDto>,
    /// The message already carries everything the turn may use: no retrieval of
    /// any kind runs, no tools are offered, and nothing is verified against
    /// sources. Backend callers only — it is never deserialized, so the
    /// frontend can neither set nor see it.
    ///
    /// `knowledge_base: false` does not mean this. It only declines to *force*
    /// a vault search; the router still ran one, which is how a journal
    /// synthesis of one space's chat was handed another space's documents.
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
