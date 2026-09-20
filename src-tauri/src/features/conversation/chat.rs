//! Conversation-based Chat Command
//!
//! Provides chat functionality with conversation history and context window management.
//! Follows DDD architecture with thin controller pattern.
//!
//! # Features
//!
//! - **Conversation History**: Maintains persistent chat history
//! - **Context Window Management**: Token-aware truncation (75% for context, 25% for generation)
//! - **Auto-titling**: Generates conversation titles from first message
//! - **Security**: Rate limiting, input validation, audit logging
//!
//! # Usage
//!
//! ```typescript
//! import { invoke } from '@tauri-apps/api/core';
//!
//! // Start new conversation
//! const response = await invoke<ChatResponse>('chat_with_conversation', {
//!   conversationId: null,  // null = create new
//!   message: 'What is RAG?'
//! });
//!
//! // Continue existing conversation
//! const nextResponse = await invoke<ChatResponse>('chat_with_conversation', {
//!   conversationId: response.conversationId,
//!   message: 'Can you explain more?'
//! });
//! ```

use crate::application::services::conversation_context::build_conversation_context;
use crate::domain::qa::hyde::QueryType;
use crate::features::conversation::compaction;
use crate::features::conversation::dto::CreateConversationRequestDto;
use crate::features::qa::dto::SourceDto;
use crate::features::settings::dto::{
    CustomToolSettingsDto, LLMPromptSettingsDto, RouterSettingsDto,
};
use crate::infrastructure::services::router::{RouterAction, RouterInput, RouterService};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use crate::shared::text_utils::extract_highlight_terms;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tracing::{info, warn};

mod cancellation;
mod fetch_memory;
mod focus;
// Public so the no-tools retrieval path and the turn's tool-selection wiring can
// reach it without this module re-exporting its whole surface.
pub mod history_tools;
pub mod memory_context;
mod persistence;
mod prompting;
// Public so the retrieval evaluation harness can reuse the pipeline's own
// sufficiency judgement instead of reimplementing it.
pub mod retrieval;
mod tool_loop;
pub mod turn_record;
mod verification;
mod web_steps;

use self::cancellation::{begin_turn, finish_turn, is_cancel_requested};
use self::focus::FocusScope;
use self::persistence::{
    finalize_successful_turn, mark_user_message_failed, persist_user_message_pending,
};
use self::prompting::{build_kb_context, enforce_numeric_citation_format, PromptMessageBuilder};
pub use self::retrieval::RetrievalSubTimingMetrics;
use self::retrieval::WEB_SOURCE_PREFIX;
use self::retrieval::{
    assign_citation_ids, citation_ids_by_chunk, confine_document_context, deduplicate_sources,
    load_recent_document_metadata, run_retrieval_pipeline, RouterDecisionOutcome,
};
use self::tool_loop::run_agentic_tool_loop;
pub use self::tool_loop::ToolLoopTimingMetrics;
use self::turn_record::TurnRecorder;
pub use self::turn_record::{
    TurnModelDto, TurnRecordDto, TurnRouterDto, TurnStepDto, TurnStepKind, TurnStepState,
    TurnTimingDto, TurnTokensDto,
};
use self::verification::{GroundingReport, GroundingVerifier};

pub fn cancel_generation_for_conversation(conversation_id: &str, request_id: Option<&str>) -> bool {
    cancellation::request_cancel(conversation_id, request_id)
}

struct TurnCancellationGuard {
    request_id: String,
}

impl TurnCancellationGuard {
    fn start(request_id: String, conversation_id: &str) -> Result<Self> {
        if !begin_turn(&request_id, conversation_id) {
            return Err(AppError::InvalidState(
                "A generation with this request ID is already in flight.".to_string(),
            ));
        }
        Ok(Self { request_id })
    }
}

impl Drop for TurnCancellationGuard {
    fn drop(&mut self) {
        finish_turn(&self.request_id);
    }
}

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
struct TraceCounts {
    /// Passages from documents in the user's vault.
    passages: usize,
    /// Distinct vault documents those passages came from.
    files: usize,
    /// Distinct web pages, which are not files and are never counted as any.
    web_pages: usize,
}

/// Split a turn's sources into vault documents and web pages.
///
/// Web results are shaped like document sources so citations can treat them
/// alike, and the counts used to be taken over the whole list. A web-only
/// answer in a space containing no documents therefore announced "10 passages
/// from 10 files" — a claim to have read ten of the user's documents, made by a
/// turn that never touched the vault. The prefix on the id is what separates
/// them.
fn trace_counts(sources: &[SourceDto]) -> TraceCounts {
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
}

impl ChatStreamEventDto {
    pub(super) fn new(conversation_id: &str, request_id: &str) -> Self {
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

#[derive(Debug, Clone, Copy)]
struct SearchFlags {
    force_kb_search: bool,
    force_web_search: bool,
    force_wiki_search: bool,
    deep_research_mode: bool,
    force_followup_mode: bool,
    closed_book: bool,
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

impl SearchFlags {
    fn from_preferences(tool_preferences: Option<&ToolPreferences>) -> Self {
        let tool_preferences = tool_preferences.cloned().unwrap_or_default();
        let normalized_turn_mode = tool_preferences
            .turn_mode
            .as_deref()
            .map(str::trim)
            .filter(|mode| !mode.is_empty())
            .map(|mode| mode.to_ascii_lowercase());
        let turn_mode_is_followup = normalized_turn_mode.as_deref() == Some("followup");
        let turn_mode_is_query = normalized_turn_mode.as_deref() == Some("query");
        let force_wiki_search = tool_preferences
            .enabled_tools
            .as_ref()
            .is_some_and(|tools| {
                tools
                    .iter()
                    .any(|name| WIKI_OPTIONAL_TOOL_NAMES.contains(&name.as_str()))
            });
        if tool_preferences.closed_book {
            // Closed wins over every other preference, so no combination of
            // flags can reopen the vault or the network for such a turn.
            return Self {
                force_kb_search: false,
                force_web_search: false,
                force_wiki_search: false,
                deep_research_mode: false,
                force_followup_mode: tool_preferences.followup_mode || turn_mode_is_followup,
                closed_book: true,
            };
        }
        Self {
            force_kb_search: tool_preferences.knowledge_base || turn_mode_is_query,
            force_web_search: tool_preferences.web_search || turn_mode_is_query,
            force_wiki_search,
            deep_research_mode: tool_preferences.deep_research_mode,
            force_followup_mode: tool_preferences.followup_mode || turn_mode_is_followup,
            closed_book: false,
        }
    }
}

/// Chat with conversation context
///
/// Maintains persistent conversation history with token-aware context window management.
/// Automatically creates new conversations or continues existing ones.
///
/// # Arguments
///
/// * `container` - DI container with services and security context
/// * `conversation_id` - Optional conversation ID (None = create new)
/// * `message` - User's message
///
/// # Returns
///
/// * `Ok(ChatResponse)` - Response with conversation ID and generated message
/// * `Err(AppError)` - If rate limited, validation fails, or generation fails
///
/// # Errors
///
/// * `AppError::NoActiveModel` - No chat model is active
/// * `AppError::RateLimitExceeded` - Too many requests (10/min)
/// * `AppError::InvalidInput` - Empty or invalid message
/// * `AppError::Database` - Database operation failed
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: 10 requests per minute
/// - **Input Validation**: Message length and content validation
/// - **Audit Logging (CWE-778)**: Logs chat interactions with context size
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface ChatResponse {
///   conversationId: string;
///   message: string;
///   contextUsed: number;
/// }
///
/// // Start new conversation
/// const response = await invoke<ChatResponse>('chat_with_conversation', {
///   conversationId: null,
///   message: 'What is semantic search?'
/// });
///
/// console.log(`Conversation ID: ${response.conversationId}`);
/// console.log(`Response: ${response.message}`);
/// console.log(`Context messages: ${response.contextUsed}`);
///
/// // Continue conversation
/// const followUp = await invoke<ChatResponse>('chat_with_conversation', {
///   conversationId: response.conversationId,
///   message: 'How does it differ from keyword search?'
/// });
///
/// console.log(`Follow-up: ${followUp.message}`);
/// console.log(`Context messages: ${followUp.contextUsed}`);
/// ```
pub async fn chat_with_conversation_impl<R: tauri::Runtime>(
    container: &Container,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    request_id: Option<String>,
    window: tauri::Window<R>,
) -> Result<ChatResponse> {
    if cancel_only.unwrap_or(false) {
        let conv_id = conversation_id.ok_or_else(|| {
            AppError::InvalidInput("Conversation ID is required to cancel generation".to_string())
        })?;
        let cancelled = cancel_generation_for_conversation(&conv_id, request_id.as_deref());
        return Ok(ChatResponse {
            conversation_id: conv_id,
            message: if cancelled {
                "cancelled".to_string()
            } else {
                "idle".to_string()
            },
            messages: Vec::new(),
            context_used: 0,
            sources: Vec::new(),
            timing_metrics: None,
        });
    }

    // DIAGNOSTIC: Log command start
    tracing::info!(
        conversation_id = conversation_id.as_deref().unwrap_or("NEW"),
        content_length = message.len(),
        "chat_with_conversation: START"
    );

    let turn_id = request_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    // Register the turn before any of the slow work below it — request
    // validation, a cold model load, opening the conversation. The composer
    // shows Stop from the moment it sends, so a turn that only becomes
    // cancellable once that work is done leaves every stop press in the
    // meantime with nothing to act on. When the caller named the conversation
    // (every store caller does) the turn registers right here, under the id
    // exactly as the caller wrote it — the same id `get_or_create_conversation_id`
    // hands back and the same one a cancel arrives with, which is what lets the
    // registry match the three up. Only a brand-new conversation has to wait
    // until its id exists.
    let mut turn_guard = match conversation_id.as_deref() {
        Some(id) if !id.trim().is_empty() => {
            Some(TurnCancellationGuard::start(turn_id.clone(), id)?)
        }
        _ => None,
    };
    let cancellation_error = || AppError::InvalidState("Generation cancelled by user.".to_string());

    let flow_start = Instant::now();
    let mut flow_metrics = ConversationFlowTimingMetrics::default();

    let validate_start = Instant::now();
    let validated_message = validate_and_guard_chat_request(container, &message).await?;
    flow_metrics.validate_request_ms = elapsed_ms(validate_start);
    if is_cancel_requested(&turn_id) {
        return Err(cancellation_error());
    }
    let search_flags = SearchFlags::from_preferences(tool_preferences.as_ref());

    info!(
        force_kb_search = search_flags.force_kb_search,
        force_web_search = search_flags.force_web_search,
        force_wiki_search = search_flags.force_wiki_search,
        deep_research_mode = search_flags.deep_research_mode,
        force_followup_mode = search_flags.force_followup_mode,
        "chat_with_conversation: tool preferences"
    );

    let llm_start = Instant::now();
    // A cold load is the longest stretch of the turn. It is not raced against
    // the cancellation flag: dropping it mid-flight would abandon a half-started
    // sidecar. The turn is registered by now, so a stop press during the load is
    // recorded and acted on the moment it returns — before any generation.
    let llm = container.get_or_load_llm().await?;
    flow_metrics.load_llm_ms = elapsed_ms(llm_start);

    let conversation_init_start = Instant::now();
    let conv_id =
        get_or_create_conversation_id(container, conversation_id, &validated_message, &llm).await?;
    if turn_guard.is_none() {
        turn_guard = Some(TurnCancellationGuard::start(turn_id.clone(), &conv_id)?);
    }
    let _turn_guard = turn_guard;
    flow_metrics.conversation_init_ms = elapsed_ms(conversation_init_start);
    if is_cancel_requested(&turn_id) {
        return Err(cancellation_error());
    }

    // Both emits and accumulates: everything below reports what it is doing
    // through this, and the same list is what the finished answer carries.
    let recorder = TurnRecorder::new(&window, &conv_id, &turn_id);
    if search_flags.deep_research_mode {
        recorder.note(
            turn_record::TurnStepKind::DeepResearch,
            "Deep research",
            None,
            Some("searches wider, follows up on what it finds, and may go back for more".into()),
        );
    }
    // Resolved once, against the conversation's space scope. A request may
    // narrow what the turn reads; it may never widen it.
    let focus = FocusScope::resolve(
        container,
        &conv_id,
        tool_preferences
            .as_ref()
            .and_then(|preferences| preferences.focus_document_ids.as_ref()),
        search_flags.closed_book,
    )
    .await;
    // A focus that survived the intersection is an explicit instruction to read
    // those documents, so the vault is searched whether or not the router would
    // have asked for it. A focus that survived *nothing* forces the search too,
    // because that is the only path that reports why the turn read nothing —
    // skipping it would leave a fail-closed turn silently indistinguishable
    // from one that simply had no reason to search.
    let search_flags = SearchFlags {
        force_kb_search: search_flags.force_kb_search || focus.is_requested(),
        ..search_flags
    };

    let conv_service = container.conversation_service();

    let settings_start = Instant::now();
    let settings = container
        .get_settings_use_case()
        .execute()
        .await
        .unwrap_or_default();
    flow_metrics.settings_load_ms = elapsed_ms(settings_start);
    let prompt_settings = normalize_prompt_settings(settings.llm.prompts.clone());
    let tool_output_settings = settings.llm.tool_output.clone();
    let router_settings = settings.llm.router.clone();
    let highlight_terms = extract_highlight_terms(
        &validated_message,
        tool_output_settings.highlight_terms_max as usize,
    );

    let max_tokens = llm.max_context_tokens();
    let context_build_start = Instant::now();
    let (context, conversation_document_context, linked_web_sources_context) =
        build_conversation_context(
            container.conversation_context(),
            container.conversation_history(),
            &conv_id,
            &llm,
            max_tokens,
            &prompt_settings.system_prompt,
        )
        .await?;
    flow_metrics.context_build_ms = elapsed_ms(context_build_start);
    let conversation_document_context =
        confine_document_context(container, &conv_id, conversation_document_context).await;
    // Pages linked to the conversation are outside material too.
    let linked_web_sources_context =
        linked_web_sources_context.filter(|_| !search_flags.closed_book);

    let router_start = Instant::now();
    let (router_decision, router_record) = resolve_router_decision(
        container,
        &conversation_document_context,
        &validated_message,
        search_flags,
        &router_settings,
        &recorder,
    )
    .await?;
    flow_metrics.router_ms = elapsed_ms(router_start);
    if is_cancel_requested(&turn_id) {
        return Err(cancellation_error());
    }

    // DIAGNOSTIC: Log context info before LLM generation
    tracing::info!(
        conversation_id = conv_id.as_str(),
        context_messages = context.len(),
        max_tokens = max_tokens,
        "chat_with_conversation: Context built, starting LLM generation"
    );

    let retrieval_start = Instant::now();
    let mut retrieval = run_retrieval_pipeline(
        container,
        &conv_service,
        &conv_id,
        &turn_id,
        &validated_message,
        &llm,
        &router_settings,
        &router_decision,
        search_flags,
        &conversation_document_context,
        &highlight_terms,
        &context,
        max_tokens,
        &tool_output_settings,
        &settings.search,
        &focus,
        &recorder,
    )
    .await;
    flow_metrics.retrieval_pipeline_ms = elapsed_ms(retrieval_start);
    flow_metrics.retrieval_subtimings = Some(retrieval.sub_timings.clone());
    if is_cancel_requested(&turn_id) {
        return Err(cancellation_error());
    }

    let prompt_build_start = Instant::now();
    // Fetched web pages are carried whole where the window allows, so they are
    // no longer small enough to ignore: what they fill is not there for the
    // user's own passages.
    let web_context_tokens = retrieval
        .web_context
        .as_deref()
        .map(|text| llm.count_tokens(text))
        .unwrap_or(0);
    let budgeted_results = budget_search_results_for_prompt(
        &retrieval.search_response.results,
        retrieval
            .available_for_rag
            .saturating_sub(web_context_tokens),
        &llm,
    );
    if budgeted_results.len() < retrieval.search_response.results.len() {
        let rag_tokens_used: usize = budgeted_results
            .iter()
            .map(|result| llm.count_tokens(&result.content))
            .sum();
        info!(
            kept = budgeted_results.len(),
            total = retrieval.search_response.results.len(),
            tokens_used = rag_tokens_used,
            budget = retrieval.available_for_rag,
            "RAG context truncated by token budget"
        );
    }

    let followup_context_text =
        if let Some((context_text, followup_sources)) = retrieval.followup_context.take() {
            info!(
                conversation_id = conv_id.as_str(),
                "Follow-up detected: reusing conversation document context"
            );
            retrieval.sources = deduplicate_sources(followup_sources);
            Some(context_text)
        } else {
            None
        };

    // Number the sources *after* every step that can reorder or replace the
    // list (including the follow-up swap above), then build the prompt from
    // those same numbers. Assigning earlier would let the follow-up path
    // renumber behind the prompt's back.
    assign_citation_ids(&mut retrieval.sources);

    // Emitted before generation so the frontend can show retrieval progress.
    // so the UI can show its reading before its writing, and persisted into the
    // assistant message metadata so it survives a reload. Nothing is emitted
    // when KB retrieval never ran — an absent trace is not a trace of zeros.
    let mut retrieval_trace = if retrieval.searched_documents > 0
        || !retrieval.sources.is_empty()
        || retrieval.kb_unavailable_reason.is_some()
    {
        let TraceCounts {
            passages,
            files,
            web_pages,
        } = trace_counts(&retrieval.sources);
        let trace = RetrievalTraceDto {
            searched_documents: retrieval.searched_documents,
            passages,
            files,
            web_pages,
            scope: if retrieval.scope_is_linked {
                "linked"
            } else {
                "vault"
            }
            .to_string(),
            unavailable_reason: retrieval.kb_unavailable_reason.clone(),
            // Only reported when the check actually ran. A turn that never
            // searched the vault has no verdict, and zeros would read as one.
            kb_sufficient: retrieval.sufficiency.as_ref().map(|v| v.sufficient),
            kb_corrective_retries: retrieval
                .sufficiency
                .as_ref()
                .map(|_| retrieval.sub_timings.kb_corrective_retries),
            kb_planner_skipped: retrieval
                .sufficiency
                .as_ref()
                .map(|_| retrieval.sub_timings.kb_planner_skipped),
            sufficiency_reasons: retrieval
                .sufficiency
                .as_ref()
                .map(|v| v.reasons.iter().map(|r| (*r).to_owned()).collect())
                .unwrap_or_default(),
            focused_documents: focus.reported_count(),
        };
        if let Err(e) = window.emit(
            "llm-stream",
            ChatStreamEventDto {
                status: Some("retrieval".to_owned()),
                retrieval: Some(trace.clone()),
                ..ChatStreamEventDto::new(&conv_id, &turn_id)
            },
        ) {
            warn!(error = %e, "Failed to emit retrieval trace");
        }
        Some(trace)
    } else {
        None
    };

    let kb_context = build_kb_context(
        &budgeted_results,
        &citation_ids_by_chunk(&retrieval.sources),
    );
    let has_linked_web_sources_context = linked_web_sources_context.is_some();
    let retrieval_web_context = retrieval.web_context.is_some();
    let has_grounded_context = followup_context_text.is_some()
        || kb_context.is_some()
        || retrieval_web_context
        || has_linked_web_sources_context;

    let enhanced_message = PromptMessageBuilder::new(
        &prompt_settings,
        &validated_message,
        retrieval.interpretation.query_type == QueryType::Greeting,
        search_flags,
    )
    .with_followup_context(followup_context_text)
    .with_kb_context(kb_context)
    .with_linked_web_sources_context(linked_web_sources_context)
    .with_web_context(retrieval.web_context.clone())
    .with_web_search_error(retrieval.web_search_error.clone())
    .with_kb_unavailable_reason(retrieval.kb_unavailable_reason.clone())
    .with_kb_sufficiency(retrieval.sufficiency.as_ref())
    .build();
    flow_metrics.prompt_build_ms = elapsed_ms(prompt_build_start);

    let persist_user_message_start = Instant::now();
    let (user_message_id, message_tokens) =
        persist_user_message_pending(&conv_service, &conv_id, &validated_message, &llm).await?;
    flow_metrics.persist_user_message_ms = elapsed_ms(persist_user_message_start);

    let tool_prep_start = Instant::now();
    let supports_tools = llm.supports_tool_calling();
    let tool_definitions = build_llm_tool_definitions(
        container,
        &llm,
        tool_preferences.as_ref(),
        &settings.llm.custom_tools,
    );
    let force_tools_for_turn = tool_preferences
        .as_ref()
        .and_then(|preferences| preferences.turn_mode.as_deref())
        .map(str::trim)
        .is_some_and(|mode| mode.eq_ignore_ascii_case("query"));
    // Initial retrieval is a candidate set, not proof that it can answer the
    // question. Keep scoped document reads/searches available for recovery, and
    // — when the turn carries web results — the ability to open one of them.
    // Retrieval reads the top pages itself, but it cites more URLs than it
    // reads; without this the model can see a link that plainly holds the
    // answer and have no way to follow it.
    let grounded_tools: Vec<_> = tool_definitions
        .iter()
        .filter(|tool| {
            matches!(tool.name.as_str(), "semantic_search" | "get_document")
                || (tool.name == "fetch_url_content"
                    && (retrieval_web_context || has_linked_web_sources_context))
        })
        .cloned()
        .collect();
    let selected_tools = if has_grounded_context && !force_tools_for_turn {
        &grounded_tools
    } else {
        &tool_definitions
    };
    // A closed-book turn loses every external tool — offering it `semantic_search`
    // would reopen the vault one tool call later — but it keeps read-only access
    // to its own transcript. This conversation is internal task context, not an
    // external source, so a turn that may not search the web must still be able
    // to recover a requirement the user stated twenty turns ago.
    //
    // Gated on `supports_tools` because a model without tool calling must not be
    // handed a schema it cannot use: telling it to call a tool that was never
    // supplied is its own failure mode.
    let turn_tools =
        history_tools::tools_for_turn(selected_tools, search_flags.closed_book, supports_tools);
    let tools_ref = (!turn_tools.is_empty()).then_some(turn_tools.as_slice());
    flow_metrics.tool_prep_ms = elapsed_ms(tool_prep_start);

    info!(
        provider = llm.provider_name(),
        supports_tools = supports_tools,
        tool_count = turn_tools.len(),
        history_tools_offered = turn_tools
            .iter()
            .filter(|tool| history_tools::is_history_tool(&tool.name))
            .count(),
        tools_enabled_for_turn = tools_ref.is_some(),
        force_tools_for_turn = force_tools_for_turn,
        has_grounded_context = has_grounded_context,
        "LLM capability check for conversation"
    );

    // Enabled memory must account for unprocessed history before generation,
    // including conversations that have no extracted ledger yet.
    let memory_plan_start = Instant::now();
    let repository = crate::features::conversation::repository::ConversationRepository::new(
        container.db_pool().clone(),
    );
    let tool_schema_tokens: usize = tools_ref
        .map(|tools| {
            tools
                .iter()
                .map(|tool| llm.count_tokens(&tool.name) + llm.count_tokens(&tool.description))
                .sum()
        })
        .unwrap_or(0);
    let memory_turn = memory_context::prepare_memory_turn(
        || async {
            let turn = memory_context::build_memory_plan(
                &repository,
                &repository,
                &llm,
                &conv_id,
                &prompt_settings.system_prompt,
                &enhanced_message,
                tool_schema_tokens,
                Vec::new(),
                settings.llm.bounded_conversation_memory,
            )
            .await?;

            if let Some(turn) = &turn {
                info!(
                    conversation_id = conv_id.as_str(),
                    memory_revision = turn.plan.memory_revision,
                    active_mandatory = turn.plan.accounting.active_mandatory_count,
                    conflicts = turn.plan.accounting.active_conflict_count,
                    total_input_tokens = turn.plan.accounting.total_input,
                    input_budget = turn.plan.accounting.input_budget,
                    accounting = ?turn.plan.accounting.accounting_method,
                    recall_passages = turn.plan.retrieval.passages_selected,
                    recall_error = ?turn.plan.retrieval.index_error,
                    compaction_required = turn.plan.compaction_required,
                    evicted = ?turn.plan.accounting.evicted,
                    elapsed_ms = elapsed_ms(memory_plan_start),
                    "Assembled bounded conversation memory for this turn"
                );
            }
            Ok(turn)
        },
        || compaction::compact_for_turn(container, conv_id.as_str()),
    )
    .await;

    let mut sources = retrieval.sources;
    let short_circuit_response = retrieval.short_circuit_response.take();
    let generation_start = Instant::now();
    let mut turn_tokens = TurnTokensDto::default();
    // Route preparation failures through normal turn cleanup, so the saved
    // user message becomes retryable rather than remaining pending forever.
    let response_result: Result<String> = match memory_turn {
        Err(error) => Err(error),
        Ok(memory_turn) => {
            if let Some(response) = short_circuit_response {
                flow_metrics.generation_subtimings = Some(ToolLoopTimingMetrics::default());
                Ok(response)
            } else {
                match run_agentic_tool_loop(
                    container,
                    &conv_service,
                    &conv_id,
                    &turn_id,
                    &llm,
                    &window,
                    &context,
                    &enhanced_message,
                    &validated_message,
                    &prompt_settings,
                    &highlight_terms,
                    &tool_output_settings,
                    &mut sources,
                    &mut retrieval_trace,
                    tools_ref,
                    generation_time_budget(search_flags),
                    std::mem::take(&mut retrieval.pages_read),
                    &focus,
                    &recorder,
                    memory_turn.as_ref().map(|turn| &turn.plan),
                )
                .await
                {
                    Ok(tool_loop_outcome) => {
                        flow_metrics.generation_subtimings = Some(tool_loop_outcome.timings);
                        turn_tokens = TurnTokensDto {
                            completion: tool_loop_outcome.output_tokens,
                            context_used: tool_loop_outcome.input_tokens,
                        };
                        Ok(tool_loop_outcome.response)
                    }
                    Err(e) => Err(e),
                }
            }
        }
    };
    flow_metrics.generation_ms = elapsed_ms(generation_start);

    match response_result {
        Ok(response) => {
            // DIAGNOSTIC: Log successful generation
            tracing::info!(
                conversation_id = conv_id.as_str(),
                response_length = response.len(),
                "chat_with_conversation: LLM generation successful"
            );

            // Some providers can terminate a stream without yielding content.
            // Persist a friendly fallback message instead of failing the whole turn.
            let assistant_response = if response.trim().is_empty() {
                warn!(
                    conversation_id = conv_id.as_str(),
                    "LLM returned empty response; using fallback message"
                );
                "I couldn't generate a response for that turn. Please try rephrasing your question."
                    .to_string()
            } else {
                response
            };
            let verification_start = Instant::now();
            // A closed-book turn cites nothing, so there is nothing to ground
            // it against — judging it only reports every sentence unsupported.
            let verification_enabled =
                settings.llm.verification.enabled && !search_flags.closed_book;
            let verify_step = verification_enabled
                .then(|| recorder.begin(TurnStepKind::Verify, "Checking the answer", None));
            let grounding_report = if verification_enabled {
                // The utility model judges claims so a chat turn is not charged a
                // second pass through the large model. Without one configured the
                // chat LLM stands in, exactly as retrieval planning does; only a
                // hard load failure drops back to lexical-only verification.
                let judge_llm = match container.get_or_load_utility_llm().await {
                    Ok(Some(utility)) => Some(utility),
                    Ok(None) => Some(Arc::clone(&llm)),
                    Err(e) => {
                        warn!(
                            error = %e,
                            "Utility LLM load failed — grounding stays lexical for this turn"
                        );
                        None
                    }
                };
                GroundingVerifier::new(judge_llm)
                    .verify(&assistant_response, &sources)
                    .await
            } else {
                GroundingReport::default()
            };
            if let Some(step) = verify_step {
                recorder.end(
                    &step,
                    TurnStepState::Done,
                    Some(verification_result_line(&grounding_report)),
                );
            }
            if verification_enabled && grounding_report.claims_evaluated > 0 {
                info!(
                    conversation_id = conv_id.as_str(),
                    claims_evaluated = grounding_report.claims_evaluated,
                    supported_claims = grounding_report.supported_claims,
                    unsupported_claims = grounding_report.unsupported_count(),
                    contradicted_claims = grounding_report.contradicted_count(),
                    judged_claims = grounding_report.judged_claim_count(),
                    grounded_ratio = grounding_report.grounded_ratio(),
                    verification_ms = elapsed_ms(verification_start),
                    "Response grounding verification complete"
                );
            }
            let verification_metadata = if verification_enabled {
                Some(grounding_report.metadata_json())
            } else {
                Some(serde_json::json!({
                    "enabled": false
                }))
            };
            flow_metrics.verification_ms = elapsed_ms(verification_start);

            // Built here rather than after persistence so `totalMs` is time to
            // the answer the reader is about to see, and so the step list is
            // exactly what was streamed.
            if turn_tokens.completion.is_none() {
                turn_tokens.completion = Some(llm.count_tokens(&assistant_response) as u64);
            }
            let turn_record = TurnRecordDto {
                model: Some(TurnModelDto {
                    id: llm.model_name().to_string(),
                    name: llm.model_name().to_string(),
                }),
                steps: recorder.steps(),
                timing: TurnTimingDto {
                    total_ms: elapsed_ms(flow_start),
                    router_ms: flow_metrics.router_ms,
                    retrieval_ms: flow_metrics.retrieval_pipeline_ms,
                    generation_ms: flow_metrics.generation_ms,
                    verification_ms: flow_metrics.verification_ms,
                    tool_ms: generation_subtimings_or_default(&flow_metrics).tool_execution_ms,
                },
                tokens: turn_tokens,
                router: router_record,
            };

            let finalize_start = Instant::now();
            let mut chat_response = finalize_successful_turn(
                container,
                &conv_service,
                &conv_id,
                &user_message_id,
                &validated_message,
                assistant_response,
                context.len(),
                sources,
                verification_metadata,
                retrieval_trace.clone(),
                Some(turn_record),
                message_tokens,
                &llm,
            )
            .await?;
            flow_metrics.finalize_persistence_ms = elapsed_ms(finalize_start);
            flow_metrics.total_ms = elapsed_ms(flow_start);
            let retrieval_sub = retrieval_subtimings_or_default(&flow_metrics);
            let generation_sub = generation_subtimings_or_default(&flow_metrics);

            info!(
                conversation_id = conv_id.as_str(),
                validate_request_ms = flow_metrics.validate_request_ms,
                load_llm_ms = flow_metrics.load_llm_ms,
                conversation_init_ms = flow_metrics.conversation_init_ms,
                settings_load_ms = flow_metrics.settings_load_ms,
                context_build_ms = flow_metrics.context_build_ms,
                router_ms = flow_metrics.router_ms,
                retrieval_pipeline_ms = flow_metrics.retrieval_pipeline_ms,
                retrieval_sub_total_ms = retrieval_sub.total_ms,
                retrieval_sub_kb_total_ms = retrieval_sub.kb_total_ms,
                retrieval_sub_kb_scope_load_ms = retrieval_sub.kb_scope_load_ms,
                retrieval_sub_kb_hyde_interpretation_ms = retrieval_sub.kb_hyde_interpretation_ms,
                retrieval_sub_kb_search_plan_ms = retrieval_sub.kb_search_plan_ms,
                retrieval_sub_kb_shortlist_planning_ms = retrieval_sub.kb_shortlist_planning_ms,
                retrieval_sub_kb_query_execution_ms = retrieval_sub.kb_query_execution_ms,
                retrieval_sub_kb_merge_shortlist_gate_ms = retrieval_sub.kb_merge_shortlist_gate_ms,
                retrieval_sub_kb_post_filters_ms = retrieval_sub.kb_post_filters_ms,
                retrieval_sub_kb_rerank_ms = retrieval_sub.kb_rerank_ms,
                retrieval_sub_kb_build_sources_ms = retrieval_sub.kb_build_sources_ms,
                retrieval_sub_kb_persist_references_ms = retrieval_sub.kb_persist_references_ms,
                retrieval_sub_external_hyde_interpretation_ms =
                    retrieval_sub.external_hyde_interpretation_ms,
                retrieval_sub_wiki_search_ms = retrieval_sub.wiki_search_ms,
                retrieval_sub_web_search_ms = retrieval_sub.web_search_ms,
                prompt_build_ms = flow_metrics.prompt_build_ms,
                persist_user_message_ms = flow_metrics.persist_user_message_ms,
                tool_prep_ms = flow_metrics.tool_prep_ms,
                generation_ms = flow_metrics.generation_ms,
                generation_sub_total_ms = generation_sub.total_ms,
                generation_sub_iterations = generation_sub.iterations,
                generation_sub_llm_stream_ms = generation_sub.llm_stream_ms,
                generation_sub_tool_execution_ms = generation_sub.tool_execution_ms,
                generation_sub_tool_call_count = generation_sub.tool_call_count,
                generation_sub_tool_success_count = generation_sub.tool_success_count,
                generation_sub_tool_failure_count = generation_sub.tool_failure_count,
                generation_sub_empty_response_retries = generation_sub.empty_response_retries,
                generation_sub_followup_prompt_build_ms = generation_sub.followup_prompt_build_ms,
                verification_ms = flow_metrics.verification_ms,
                finalize_persistence_ms = flow_metrics.finalize_persistence_ms,
                total_ms = flow_metrics.total_ms,
                "chat_with_conversation: flow timing metrics"
            );

            chat_response.timing_metrics = Some(flow_metrics.clone());
            Ok(chat_response)
        }
        Err(e) => {
            // DIAGNOSTIC: Log generation failure with full error details
            tracing::error!(
                conversation_id = conv_id.as_str(),
                error = %e,
                "chat_with_conversation: LLM generation FAILED"
            );

            mark_user_message_failed(&conv_service, &user_message_id).await;

            flow_metrics.total_ms = elapsed_ms(flow_start);
            let retrieval_sub = retrieval_subtimings_or_default(&flow_metrics);
            let generation_sub = generation_subtimings_or_default(&flow_metrics);
            tracing::error!(
                conversation_id = conv_id.as_str(),
                validate_request_ms = flow_metrics.validate_request_ms,
                load_llm_ms = flow_metrics.load_llm_ms,
                conversation_init_ms = flow_metrics.conversation_init_ms,
                settings_load_ms = flow_metrics.settings_load_ms,
                context_build_ms = flow_metrics.context_build_ms,
                router_ms = flow_metrics.router_ms,
                retrieval_pipeline_ms = flow_metrics.retrieval_pipeline_ms,
                retrieval_sub_total_ms = retrieval_sub.total_ms,
                retrieval_sub_kb_total_ms = retrieval_sub.kb_total_ms,
                retrieval_sub_kb_scope_load_ms = retrieval_sub.kb_scope_load_ms,
                retrieval_sub_kb_hyde_interpretation_ms = retrieval_sub.kb_hyde_interpretation_ms,
                retrieval_sub_kb_search_plan_ms = retrieval_sub.kb_search_plan_ms,
                retrieval_sub_kb_shortlist_planning_ms = retrieval_sub.kb_shortlist_planning_ms,
                retrieval_sub_kb_query_execution_ms = retrieval_sub.kb_query_execution_ms,
                retrieval_sub_kb_merge_shortlist_gate_ms = retrieval_sub.kb_merge_shortlist_gate_ms,
                retrieval_sub_kb_post_filters_ms = retrieval_sub.kb_post_filters_ms,
                retrieval_sub_kb_rerank_ms = retrieval_sub.kb_rerank_ms,
                retrieval_sub_kb_build_sources_ms = retrieval_sub.kb_build_sources_ms,
                retrieval_sub_kb_persist_references_ms = retrieval_sub.kb_persist_references_ms,
                retrieval_sub_external_hyde_interpretation_ms =
                    retrieval_sub.external_hyde_interpretation_ms,
                retrieval_sub_wiki_search_ms = retrieval_sub.wiki_search_ms,
                retrieval_sub_web_search_ms = retrieval_sub.web_search_ms,
                prompt_build_ms = flow_metrics.prompt_build_ms,
                persist_user_message_ms = flow_metrics.persist_user_message_ms,
                tool_prep_ms = flow_metrics.tool_prep_ms,
                generation_ms = flow_metrics.generation_ms,
                generation_sub_total_ms = generation_sub.total_ms,
                generation_sub_iterations = generation_sub.iterations,
                generation_sub_llm_stream_ms = generation_sub.llm_stream_ms,
                generation_sub_tool_execution_ms = generation_sub.tool_execution_ms,
                generation_sub_tool_call_count = generation_sub.tool_call_count,
                generation_sub_tool_success_count = generation_sub.tool_success_count,
                generation_sub_tool_failure_count = generation_sub.tool_failure_count,
                generation_sub_empty_response_retries = generation_sub.empty_response_retries,
                generation_sub_followup_prompt_build_ms = generation_sub.followup_prompt_build_ms,
                verification_ms = flow_metrics.verification_ms,
                finalize_persistence_ms = flow_metrics.finalize_persistence_ms,
                total_ms = flow_metrics.total_ms,
                "chat_with_conversation: flow timing metrics (failed turn)"
            );

            Err(e)
        }
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// What the `verify` step says it found.
///
/// An answer with nothing to check is not an answer that failed its check, so
/// it says so in those words rather than reporting "0 of 0 backed".
fn verification_result_line(report: &GroundingReport) -> String {
    if report.claims_evaluated == 0 {
        return "nothing to check against sources".to_string();
    }
    format!(
        "{} of {} claims backed",
        report.supported_claims, report.claims_evaluated
    )
}

/// Generation allowance for one turn, shared by tool rounds and provider retries.
/// Deep research reads many sources and reasons for much longer than a reply.
fn generation_time_budget(search_flags: SearchFlags) -> Duration {
    const CHAT: Duration = Duration::from_secs(30 * 60);
    const DEEP_RESEARCH: Duration = Duration::from_secs(2 * 60 * 60);
    if search_flags.deep_research_mode {
        DEEP_RESEARCH
    } else {
        CHAT
    }
}

fn retrieval_subtimings_or_default(
    flow_metrics: &ConversationFlowTimingMetrics,
) -> RetrievalSubTimingMetrics {
    flow_metrics
        .retrieval_subtimings
        .clone()
        .unwrap_or_default()
}

fn generation_subtimings_or_default(
    flow_metrics: &ConversationFlowTimingMetrics,
) -> ToolLoopTimingMetrics {
    flow_metrics
        .generation_subtimings
        .clone()
        .unwrap_or_default()
}

async fn validate_and_guard_chat_request(container: &Container, message: &str) -> Result<String> {
    container
        .security_context()
        .rate_limiters()
        .qa
        .check_rate_limit(message)
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let validated_message = container
        .security_context()
        .input_validator()
        .validate_search_query(message)?;
    if validated_message.trim().is_empty() {
        return Err(AppError::InvalidInput("Message cannot be empty".into()));
    }

    Ok(validated_message)
}

async fn get_or_create_conversation_id(
    container: &Container,
    conversation_id: Option<String>,
    validated_message: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
) -> Result<String> {
    match conversation_id {
        Some(id) => Ok(id),
        None => {
            let create_uc = container.create_conversation_use_case();
            let request = CreateConversationRequestDto {
                title: generate_title(validated_message),
                model_name: llm.model_name().to_string(),
                system_prompt: None,
            };
            let response = create_uc.execute(request).await?;
            Ok(response.conversation.id)
        }
    }
}

fn normalize_prompt_settings(mut prompt_settings: LLMPromptSettingsDto) -> LLMPromptSettingsDto {
    const LEGACY_GREETING_TEMPLATE_SIGNATURE: u64 = 0xa8c8_948b_d572_9258;
    const UPDATED_GREETING_TEMPLATE: &str =
        "The user greeted you: \"{question}\". Reply briefly and warmly, then offer help with documents, web search, or general questions.";
    const LEGACY_NO_CONTEXT_TEMPLATE_SIGNATURE: u64 = 0x5384_8234_c2d7_4254;
    const UPDATED_NO_CONTEXT_TEMPLATE: &str =
        "The user asked: \"{question}\"\n\nNo relevant documents were found in local documents for this turn. Respond helpfully using general knowledge when appropriate, and suggest web search or adding documents if they want sourced evidence.";
    const LEGACY_RAG_TEMPLATE_SIGNATURE: u64 = 0xa971_9544_23a3_fba2;
    const UPDATED_RAG_TEMPLATE: &str =
        "Answer the user's question using only the provided context. Cite every factual statement supported by the context using numeric brackets like [1], [2], [3]. If the excerpts are insufficient, use available document search/read tools before concluding that evidence is missing. If still unsupported, say it was not found in the excerpts searched and do not guess or claim the entire collection lacks it. Do not cite unrelated context. Do not cite a source that does not support the associated statement. If you need to call get_document, use the exact Document ID shown in the context. For long documents, request additional pages with the page parameter.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:";

    // Soft migration with stable signatures keeps custom templates intact while
    // updating only known legacy defaults.
    if stable_prompt_signature(&prompt_settings.greeting_prompt_template)
        == LEGACY_GREETING_TEMPLATE_SIGNATURE
    {
        prompt_settings.greeting_prompt_template = UPDATED_GREETING_TEMPLATE.to_string();
    }
    if stable_prompt_signature(&prompt_settings.no_context_prompt_template)
        == LEGACY_NO_CONTEXT_TEMPLATE_SIGNATURE
    {
        prompt_settings.no_context_prompt_template = UPDATED_NO_CONTEXT_TEMPLATE.to_string();
    }
    if stable_prompt_signature(&prompt_settings.rag_prompt_template)
        == LEGACY_RAG_TEMPLATE_SIGNATURE
    {
        prompt_settings.rag_prompt_template = UPDATED_RAG_TEMPLATE.to_string();
    }

    prompt_settings.system_prompt = enforce_numeric_citation_format(&prompt_settings.system_prompt);
    prompt_settings.system_prompt.push_str(
        "\n\nDocument evidence rules: When the user asks to learn from or rely only on their documents, \
         verify factual claims against retrieved text before answering, including chapter names, outlines, \
         section numbers, dates, and page references. If evidence is missing, retrieve it or say it is not \
         verified; do not fill gaps from memory. Search results are a ranked subset, not a complete inventory. \
         Relevance scores rank query matches; they do not diagnose embedding quality, document corruption, \
         or indexing completeness. Low scores alone do not establish any such problem. get_document page \
         and total_pages describe internal text pagination, not original PDF page numbers. Cite printed PDF \
         pages only when supported by the returned source text or explicit PDF page metadata. If a tool \
         fails, report that specific failure without assuming the user's documents are absent.",
    );
    prompt_settings.system_prompt.push_str(
        "\nRetrieved excerpts are an initial selection, not the entire collection. If they are insufficient, \
         use available document search/read tools with a focused query before concluding evidence is missing. \
         For learning or overview requests, explain what the supplied introduction and contents actually establish, \
         then teach one supported concept. A manual need not contain a prewritten lesson to support teaching it. \
         Distinguish 'not found in the excerpts searched' from 'not present anywhere in the documents'.",
    );
    prompt_settings.rag_prompt_template =
        enforce_numeric_citation_format(&prompt_settings.rag_prompt_template);
    prompt_settings.tool_followup_prompt_template =
        enforce_numeric_citation_format(&prompt_settings.tool_followup_prompt_template);
    prompt_settings
}

fn stable_prompt_signature(input: &str) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let normalized = input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    normalized
        .as_bytes()
        .iter()
        .fold(OFFSET_BASIS, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
        })
}

/// Route the turn, and keep what the router said about it.
///
/// The decision used to be reduced to its `action` the moment it arrived, so an
/// answer could be steered by a 0.31-confidence guess with a rationale nobody
/// ever saw. Both now reach the turn record, and the only paths that report no
/// router are the ones where no router ran.
async fn resolve_router_decision(
    container: &Container,
    conversation_document_context: &[crate::domain::conversation::DocumentReference],
    validated_message: &str,
    search_flags: SearchFlags,
    router_settings: &RouterSettingsDto,
    recorder: &TurnRecorder,
) -> Result<(RouterDecisionOutcome, Option<TurnRouterDto>)> {
    // A closed-book turn has nothing to route: the pipeline returns before it
    // reads this, and asking the router model would only spend time.
    if search_flags.force_web_search || search_flags.closed_book {
        return Ok((
            RouterDecisionOutcome {
                action: RouterAction::NewSearch,
                recent_doc_meta: None,
                clarify_message: None,
            },
            None,
        ));
    }

    let recent_doc_meta =
        load_recent_document_metadata(container, conversation_document_context).await;
    let has_recent_document = recent_doc_meta.is_some();
    if conversation_document_context.is_empty() || !router_settings.enabled || !has_recent_document
    {
        return Ok((
            RouterDecisionOutcome {
                action: RouterAction::NewSearch,
                recent_doc_meta,
                clarify_message: None,
            },
            None,
        ));
    }

    let router_input = RouterInput {
        query: validated_message.to_string(),
        has_recent_document,
        recent_document_title: recent_doc_meta.as_ref().map(|m| m.title.clone()),
        recent_document_id: recent_doc_meta.as_ref().map(|m| m.document_id.clone()),
    };
    let step = recorder.begin(TurnStepKind::Route, "Deciding how to answer", None);
    let router_llm = container.get_or_load_router_llm().await?;
    let router = RouterService::new(router_llm, router_settings.clone());
    let decision = router.route(router_input).await;
    let action = router_action_code(&decision.action);
    recorder.end(
        &step,
        TurnStepState::Done,
        Some(format!(
            "{} · {:.0}% confident",
            router_action_label(&decision.action),
            decision.confidence * 100.0
        )),
    );

    Ok((
        RouterDecisionOutcome {
            action: decision.action,
            recent_doc_meta,
            clarify_message: decision.clarify_question,
        },
        Some(TurnRouterDto {
            action,
            confidence: decision.confidence,
            rationale: decision.rationale,
        }),
    ))
}

/// The stable code the UI and tests match on.
fn router_action_code(action: &RouterAction) -> String {
    match action {
        RouterAction::UseLastDocument => "use_last_document",
        RouterAction::NewSearch => "new_search",
        RouterAction::Clarify => "clarify",
    }
    .to_string()
}

/// The same decision as a sentence, for the step's one line.
fn router_action_label(action: &RouterAction) -> &'static str {
    match action {
        RouterAction::UseLastDocument => "Continuing with the last document",
        RouterAction::NewSearch => "Searching afresh",
        RouterAction::Clarify => "Asking what you mean",
    }
}

fn budget_search_results_for_prompt<'a>(
    search_results: &'a [crate::features::search::dto::SearchResultDto],
    available_for_rag: usize,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
) -> Vec<&'a crate::features::search::dto::SearchResultDto> {
    let mut rag_tokens_used = 0usize;
    search_results
        .iter()
        .filter(|result| {
            let chunk_tokens = llm.count_tokens(&result.content);
            if rag_tokens_used + chunk_tokens > available_for_rag {
                return false;
            }
            rag_tokens_used += chunk_tokens;
            true
        })
        .collect()
}

const CORE_TOOL_NAMES: [&str; 3] = ["semantic_search", "get_document", "list_documents"];
const OPTIONAL_BUILTIN_TOOL_NAMES: [&str; 4] = [
    "web_search",
    "fetch_url_content",
    "wiki_search",
    "wiki_summary",
];
const WIKI_OPTIONAL_TOOL_NAMES: [&str; 2] = ["wiki_search", "wiki_summary"];

fn build_optional_tool_allowlist(
    tool_preferences: Option<&ToolPreferences>,
) -> Option<HashSet<String>> {
    tool_preferences
        .and_then(|preferences| preferences.enabled_tools.as_ref())
        .map(|enabled_tools| {
            enabled_tools
                .iter()
                .map(|name| name.trim())
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
}

fn build_llm_tool_definitions(
    container: &Container,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    tool_preferences: Option<&ToolPreferences>,
    custom_tools: &[CustomToolSettingsDto],
) -> Vec<crate::application::ports::ToolDefinition> {
    if !llm.supports_tool_calling() {
        return Vec::new();
    }

    let enabled_custom_tools: std::collections::HashMap<String, &CustomToolSettingsDto> =
        custom_tools
            .iter()
            .filter(|tool| tool.enabled)
            .map(|tool| (tool.name.clone(), tool))
            .collect();

    let optional_allowlist = build_optional_tool_allowlist(tool_preferences);

    let registry = container.function_registry();
    let domain_tools = registry.list_tools();
    let mut definitions: Vec<crate::application::ports::ToolDefinition> = domain_tools
        .iter()
        .filter(|tool| {
            let tool_name = tool.name.as_str();
            if CORE_TOOL_NAMES.contains(&tool_name) {
                return true;
            }

            if OPTIONAL_BUILTIN_TOOL_NAMES.contains(&tool_name) {
                return optional_allowlist
                    .as_ref()
                    .is_some_and(|allowlist| allowlist.contains(tool_name));
            }

            if !enabled_custom_tools.contains_key(tool_name) {
                // Skip stale custom tool definitions that are no longer configured.
                return false;
            }

            optional_allowlist
                .as_ref()
                .is_none_or(|allowlist| allowlist.contains(tool_name))
        })
        .map(|t| crate::application::ports::ToolDefinition {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.input_schema.clone(),
        })
        .collect();

    let existing_names: HashSet<String> =
        definitions.iter().map(|tool| tool.name.clone()).collect();

    definitions.extend(
        enabled_custom_tools
            .values()
            .filter(|tool| !existing_names.contains(&tool.name))
            .filter(|tool| {
                optional_allowlist
                    .as_ref()
                    .is_none_or(|allowlist| allowlist.contains(&tool.name))
            })
            .map(|tool| crate::application::ports::ToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Query text passed to this custom endpoint"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Requested max result count",
                            "default": tool.default_max_results,
                            "minimum": 1,
                            "maximum": 100
                        }
                    },
                    "required": ["query"]
                }),
            }),
    );

    definitions
}

/// Derive a conversation title from the user's first message.
///
/// Truncation is by **characters**, not bytes. `&message[..50]` panics when
/// byte 50 lands inside a multi-byte character, so any first message longer
/// than 50 bytes containing an accent, CJK character, or emoji took down the
/// chat command handler.
fn generate_title(message: &str) -> String {
    const MAX_CHARS: usize = 50;

    if message.chars().count() <= MAX_CHARS {
        message.to_string()
    } else {
        format!(
            "{}...",
            crate::shared::text_utils::safe_truncate(message, MAX_CHARS)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_source(document_id: &str, chunk_id: &str) -> SourceDto {
        SourceDto {
            document_id: document_id.to_string(),
            chunk_id: chunk_id.to_string(),
            content: "text".to_string(),
            score: 1.0,
            path: None,
            position: None,
            file_name: "doc.pdf".to_string(),
            file_path: "/doc.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            category: "PDF Document".to_string(),
            file_size_bytes: 1,
            modified_at: String::new(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            page_number: None,
            chunk_excerpts: None,
            citation_id: None,
        }
    }

    fn web_source(url: &str) -> SourceDto {
        SourceDto {
            document_id: format!("{WEB_SOURCE_PREFIX}{url}"),
            mime_type: "text/html".to_string(),
            category: "Web Article".to_string(),
            ..vault_source("unused", url)
        }
    }

    /// The turn that started all of this: a chat in a space holding no
    /// documents, answered entirely from the web, which reported "10 passages
    /// from 10 files" and so looked exactly like a scope leak.
    #[test]
    fn a_web_only_turn_reports_no_files_and_no_passages() {
        let sources: Vec<SourceDto> = (0..10)
            .map(|index| web_source(&format!("https://example.com/{index}")))
            .collect();
        assert_eq!(
            trace_counts(&sources),
            TraceCounts {
                passages: 0,
                files: 0,
                web_pages: 10
            }
        );
    }

    #[test]
    fn a_vault_turn_counts_documents_and_no_web_pages() {
        let sources = vec![
            vault_source("doc-a", "chunk-1"),
            vault_source("doc-a", "chunk-2"),
            vault_source("doc-b", "chunk-3"),
        ];
        assert_eq!(
            trace_counts(&sources),
            TraceCounts {
                passages: 3,
                files: 2,
                web_pages: 0
            }
        );
    }

    /// A deep-research turn reads both. Each has to be counted as what it is.
    #[test]
    fn a_mixed_turn_keeps_the_two_apart() {
        let sources = vec![
            vault_source("doc-a", "chunk-1"),
            web_source("https://example.com/one"),
            web_source("https://example.com/two"),
            web_source("https://example.com/one"),
        ];
        assert_eq!(
            trace_counts(&sources),
            TraceCounts {
                passages: 1,
                files: 1,
                web_pages: 2
            }
        );
    }

    #[test]
    fn a_turn_with_no_sources_counts_nothing() {
        assert_eq!(trace_counts(&[]), TraceCounts::default());
    }

    /// Zero web pages must not appear on the wire at all, so a trace that read
    /// only documents looks the same as one written before the field existed.
    #[test]
    fn a_trace_without_web_pages_omits_the_field() {
        let trace = RetrievalTraceDto {
            searched_documents: 3,
            passages: 2,
            files: 1,
            scope: "vault".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_value(&trace).expect("serialize");
        assert!(json.get("webPages").is_none(), "got {json}");

        let with_web = RetrievalTraceDto {
            web_pages: 4,
            ..trace
        };
        let json = serde_json::to_value(&with_web).expect("serialize");
        assert_eq!(
            json.get("webPages").and_then(serde_json::Value::as_u64),
            Some(4)
        );
    }

    /// The sufficiency fields are additive. A trace written before the check
    /// existed carries none of them, and a reader that treated a missing
    /// verdict as `false` would report every historical turn as insufficient.
    #[test]
    fn a_trace_without_a_verdict_serializes_without_the_sufficiency_fields() {
        let trace = RetrievalTraceDto {
            searched_documents: 12,
            passages: 3,
            files: 2,
            scope: "vault".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_value(&trace).expect("serialize");
        for absent in [
            "kbSufficient",
            "kbCorrectiveRetries",
            "kbPlannerSkipped",
            "sufficiencyReasons",
            "unavailableReason",
        ] {
            assert!(json.get(absent).is_none(), "{absent} should be omitted");
        }
    }

    #[test]
    fn a_corrective_pass_is_reported_with_its_reasons() {
        let trace = RetrievalTraceDto {
            searched_documents: 12,
            passages: 3,
            files: 2,
            scope: "vault".to_string(),
            kb_sufficient: Some(false),
            kb_corrective_retries: Some(1),
            kb_planner_skipped: Some(true),
            sufficiency_reasons: vec!["low_term_coverage".to_string()],
            ..Default::default()
        };
        let json = serde_json::to_value(&trace).expect("serialize");
        assert_eq!(
            json.get("kbSufficient").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            json.get("kbCorrectiveRetries").and_then(|v| v.as_u64()),
            Some(1)
        );
        assert_eq!(
            json.get("kbPlannerSkipped").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            json.get("sufficiencyReasons")
                .and_then(|v| v.as_array())
                .map(Vec::len),
            Some(1)
        );
    }

    /// Byte-slicing a string at a fixed offset panics when that offset falls
    /// inside a multi-byte character. Any first chat message over 50 bytes
    /// containing non-ASCII text used to bring down the command handler.
    #[test]
    fn generate_title_does_not_panic_on_multibyte_input() {
        let cases = [
            // Accents: 'é' is 2 bytes, so byte 50 lands mid-character.
            "Bonjour, je voudrais discuter des propriétés thermodynamiques de ce système.",
            // CJK: every character is 3 bytes.
            "这是一个很长的中文句子用来测试标题生成功能是否会因为多字节字符而崩溃。",
            // Emoji: 4 bytes each, placed to straddle the boundary.
            "Let's talk about the launch 🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀 today",
            // Mixed scripts.
            "Café ☕ meeting notes こんにちは everyone, here is the agenda for today's sync",
        ];

        for message in cases {
            let title = generate_title(message);
            assert!(
                !title.is_empty(),
                "title should not be empty for {:?}",
                message
            );
            // 50 characters plus the ellipsis.
            assert!(
                title.chars().count() <= 53,
                "title {:?} exceeded the character budget",
                title
            );
        }
    }

    #[test]
    fn generate_title_truncates_by_characters_not_bytes() {
        // 60 CJK characters = 180 bytes. A byte-based truncation would cut at
        // ~16 characters (or panic); character-based gives exactly 50.
        let message = "字".repeat(60);
        let title = generate_title(&message);
        assert_eq!(title.chars().count(), 53, "50 chars plus '...'");
        assert!(title.ends_with("..."));
    }

    #[test]
    fn test_generate_title_short() {
        let short_message = "Hello world";
        assert_eq!(generate_title(short_message), "Hello world");
    }

    #[test]
    fn test_generate_title_long() {
        let long_message =
            "This is a very long message that exceeds fifty characters and should be truncated";
        let title = generate_title(long_message);

        assert!(title.len() <= 53); // 50 + "..."
        assert!(title.ends_with("..."));
        assert_eq!(
            title,
            "This is a very long message that exceeds fifty cha..."
        );
    }

    #[test]
    fn test_generate_title_exact_length() {
        let message = "x".repeat(50);
        let title = generate_title(&message);

        assert_eq!(title.len(), 50);
        assert!(!title.ends_with("..."));
    }

    #[test]
    fn test_search_flags_respect_followup_turn_mode() {
        let prefs = ToolPreferences {
            knowledge_base: false,
            web_search: false,
            deep_research_mode: false,
            followup_mode: false,
            turn_mode: Some("followup".to_string()),
            enabled_tools: None,
            focus_document_ids: None,
            closed_book: false,
        };

        let flags = SearchFlags::from_preferences(Some(&prefs));
        assert!(!flags.force_kb_search);
        assert!(!flags.force_web_search);
        assert!(flags.force_followup_mode);
    }

    /// The reported bug: a journal synthesis asked for `knowledge_base: false`
    /// and was still handed another space's documents, because that flag only
    /// declines to force a search. Closed has to beat every other preference.
    #[test]
    fn a_closed_book_turn_cannot_be_reopened_by_any_other_preference() {
        let prefs = ToolPreferences {
            knowledge_base: true,
            web_search: true,
            deep_research_mode: true,
            followup_mode: false,
            turn_mode: Some("query".to_string()),
            enabled_tools: Some(vec!["wiki_search".to_string()]),
            focus_document_ids: None,
            closed_book: true,
        };

        let flags = SearchFlags::from_preferences(Some(&prefs));

        assert!(flags.closed_book);
        assert!(!flags.force_kb_search);
        assert!(!flags.force_web_search);
        assert!(!flags.force_wiki_search);
        assert!(!flags.deep_research_mode);
    }

    /// Only backend callers may close a turn. If the frontend could send the
    /// flag it could also clear it, so it must not survive deserialization.
    #[test]
    fn the_frontend_cannot_set_closed_book() {
        let prefs: ToolPreferences =
            serde_json::from_str(r#"{"knowledgeBase":true,"closedBook":true,"closed_book":true}"#)
                .unwrap();

        assert!(!prefs.closed_book);
    }

    #[test]
    fn test_deep_research_gets_a_longer_generation_budget() {
        let mut prefs = ToolPreferences {
            knowledge_base: false,
            web_search: true,
            deep_research_mode: false,
            followup_mode: false,
            turn_mode: None,
            enabled_tools: None,
            focus_document_ids: None,
            closed_book: false,
        };
        let chat = generation_time_budget(SearchFlags::from_preferences(Some(&prefs)));
        prefs.deep_research_mode = true;
        let research = generation_time_budget(SearchFlags::from_preferences(Some(&prefs)));
        assert!(chat >= Duration::from_secs(30 * 60));
        assert!(research >= Duration::from_secs(2 * 60 * 60));
        assert!(research > chat);
    }

    #[test]
    fn test_search_flags_query_turn_mode_forces_retrieval() {
        let prefs = ToolPreferences {
            knowledge_base: false,
            web_search: false,
            deep_research_mode: false,
            followup_mode: false,
            turn_mode: Some("query".to_string()),
            enabled_tools: None,
            focus_document_ids: None,
            closed_book: false,
        };

        let flags = SearchFlags::from_preferences(Some(&prefs));
        assert!(flags.force_kb_search);
        assert!(flags.force_web_search);
        assert!(!flags.force_followup_mode);
    }

    #[test]
    fn optional_builtin_tools_require_an_explicit_per_turn_allowlist() {
        let no_preferences = build_optional_tool_allowlist(None);
        assert!(no_preferences.is_none());
        assert!(!no_preferences
            .as_ref()
            .is_some_and(|allowlist| allowlist.contains("fetch_url_content")));

        let preferences = ToolPreferences {
            enabled_tools: Some(vec!["fetch_url_content".to_string()]),
            ..ToolPreferences::default()
        };
        let explicit = build_optional_tool_allowlist(Some(&preferences));
        assert!(explicit
            .as_ref()
            .is_some_and(|allowlist| allowlist.contains("fetch_url_content")));
    }
}
