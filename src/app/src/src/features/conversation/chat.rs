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

use crate::application::dtos::conversation_dto::CreateConversationRequestDto;
use crate::features::qa::dto::SourceDto;
use crate::application::dtos::settings::{
    CustomToolSettingsDto, LLMPromptSettingsDto, RouterSettingsDto,
};
use crate::application::services::context_window_builder::ContextWindowBuilder;
use crate::domain::qa::hyde::QueryType;
use crate::infrastructure::events::ConversationEvent;
use crate::infrastructure::services::router::{RouterAction, RouterInput, RouterService};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use crate::shared::text_utils::{extract_highlight_terms, safe_truncate};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

// Submodules live in the sibling `chat/` directory at this file's
// canonical physical location. Because `chat.rs` is loaded via a
// Strangler Fig `#[path]` redirect from interfaces/commands/mod.rs,
// Rust resolves `mod foo;` relative to the *registration site*, not
// the physical location — so each submodule needs its own `#[path]`.
#[path = "chat/cancellation.rs"]
mod cancellation;
#[path = "chat/persistence.rs"]
mod persistence;
#[path = "chat/prompting.rs"]
mod prompting;
#[path = "chat/retrieval/mod.rs"]
mod retrieval;
#[path = "chat/tool_loop.rs"]
mod tool_loop;
#[path = "chat/verification.rs"]
mod verification;

use self::cancellation::{begin_turn, finish_turn, is_cancel_requested};
use self::persistence::{
    finalize_successful_turn, mark_user_message_failed, persist_user_message_pending,
};
use self::prompting::{build_kb_context, enforce_numeric_citation_format, PromptMessageBuilder};
pub use self::retrieval::RetrievalSubTimingMetrics;
use self::retrieval::{
    deduplicate_sources, load_recent_document_metadata, run_retrieval_pipeline,
    RouterDecisionOutcome,
};
use self::tool_loop::run_agentic_tool_loop;
pub use self::tool_loop::ToolLoopTimingMetrics;
use self::verification::{verify_response_grounding_summary, GroundingReport};

pub fn cancel_generation_for_conversation(conversation_id: &str) -> bool {
    cancellation::request_cancel(conversation_id)
}

struct TurnCancellationGuard {
    conversation_id: String,
}

impl TurnCancellationGuard {
    fn start(conversation_id: &str) -> Self {
        begin_turn(conversation_id);
        Self {
            conversation_id: conversation_id.to_string(),
        }
    }
}

impl Drop for TurnCancellationGuard {
    fn drop(&mut self) {
        finish_turn(&self.conversation_id);
    }
}

/// Single conversation message for frontend
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    pub role: String, // "user" | "assistant" | "system"
    pub content: String,
    pub status: String, // "pending" | "completed" | "failed"
    pub created_at: String,
    pub metadata: Option<String>,
}

/// Chat response with conversation metadata
#[derive(Debug, Serialize)]
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

#[derive(Debug, Clone, Default, Deserialize)]
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
}

#[derive(Debug, Clone, Copy)]
struct SearchFlags {
    force_kb_search: bool,
    force_web_search: bool,
    force_wiki_search: bool,
    deep_research_mode: bool,
    force_followup_mode: bool,
}

#[derive(Debug, Serialize, Clone, Default)]
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
        Self {
            force_kb_search: tool_preferences.knowledge_base || turn_mode_is_query,
            force_web_search: tool_preferences.web_search || turn_mode_is_query,
            force_wiki_search,
            deep_research_mode: tool_preferences.deep_research_mode,
            force_followup_mode: tool_preferences.followup_mode || turn_mode_is_followup,
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
    window: tauri::Window<R>,
) -> Result<ChatResponse> {
    if cancel_only.unwrap_or(false) {
        let conv_id = conversation_id.ok_or_else(|| {
            AppError::InvalidInput("Conversation ID is required to cancel generation".to_string())
        })?;
        let cancelled = cancel_generation_for_conversation(&conv_id);
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

    let flow_start = Instant::now();
    let mut flow_metrics = ConversationFlowTimingMetrics::default();

    let validate_start = Instant::now();
    let validated_message = validate_and_guard_chat_request(container, &message).await?;
    flow_metrics.validate_request_ms = elapsed_ms(validate_start);
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
    let llm = container.get_or_load_llm().await?;
    flow_metrics.load_llm_ms = elapsed_ms(llm_start);

    let conversation_init_start = Instant::now();
    let conv_id =
        get_or_create_conversation_id(container, conversation_id, &validated_message, &llm).await?;
    let _turn_guard = TurnCancellationGuard::start(&conv_id);
    flow_metrics.conversation_init_ms = elapsed_ms(conversation_init_start);

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
            container,
            &conv_service,
            &conv_id,
            &llm,
            max_tokens,
            &prompt_settings.system_prompt,
        )
        .await?;
    trigger_background_summary_refresh_if_needed(
        container,
        &conv_service,
        &conv_id,
        &llm,
        max_tokens,
    )
    .await;
    flow_metrics.context_build_ms = elapsed_ms(context_build_start);

    let router_start = Instant::now();
    let router_decision = resolve_router_decision(
        container,
        &conversation_document_context,
        &validated_message,
        search_flags.force_web_search,
        &router_settings,
    )
    .await?;
    flow_metrics.router_ms = elapsed_ms(router_start);

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
    )
    .await;
    flow_metrics.retrieval_pipeline_ms = elapsed_ms(retrieval_start);
    flow_metrics.retrieval_subtimings = Some(retrieval.sub_timings.clone());
    let cancellation_error = || AppError::InvalidState("Generation cancelled by user.".to_string());
    if is_cancel_requested(&conv_id) {
        return Err(cancellation_error());
    }

    let prompt_build_start = Instant::now();
    let budgeted_results = budget_search_results_for_prompt(
        &retrieval.search_response.results,
        retrieval.available_for_rag,
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

    let kb_context = build_kb_context(&budgeted_results);
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
    let has_linked_web_sources_context = linked_web_sources_context.is_some();
    let has_grounded_context = followup_context_text.is_some()
        || kb_context.is_some()
        || retrieval.web_context.is_some()
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
    let tools_ref =
        if (has_grounded_context && !force_tools_for_turn) || tool_definitions.is_empty() {
            None
        } else {
            Some(tool_definitions.as_slice())
        };
    flow_metrics.tool_prep_ms = elapsed_ms(tool_prep_start);

    info!(
        provider = llm.provider_name(),
        supports_tools = supports_tools,
        tool_count = tool_definitions.len(),
        tools_enabled_for_turn = tools_ref.is_some(),
        force_tools_for_turn = force_tools_for_turn,
        has_grounded_context = has_grounded_context,
        "LLM capability check for conversation"
    );

    let mut sources = retrieval.sources;
    let short_circuit_response = retrieval.short_circuit_response.take();
    let generation_start = Instant::now();
    let response_result: Result<String> = if let Some(response) = short_circuit_response {
        flow_metrics.generation_subtimings = Some(ToolLoopTimingMetrics::default());
        Ok(response)
    } else {
        match run_agentic_tool_loop(
            container,
            &conv_service,
            &conv_id,
            &llm,
            &window,
            &context,
            &enhanced_message,
            &validated_message,
            &prompt_settings,
            &highlight_terms,
            &tool_output_settings,
            &mut sources,
            tools_ref,
        )
        .await
        {
            Ok(tool_loop_outcome) => {
                flow_metrics.generation_subtimings = Some(tool_loop_outcome.timings);
                Ok(tool_loop_outcome.response)
            }
            Err(e) => Err(e),
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
            let verification_enabled = settings.llm.verification.enabled;
            let grounding_report = if verification_enabled {
                verify_response_grounding_summary(&assistant_response, &sources)
            } else {
                GroundingReport::default()
            };
            if verification_enabled && grounding_report.claims_evaluated > 0 {
                info!(
                    conversation_id = conv_id.as_str(),
                    claims_evaluated = grounding_report.claims_evaluated,
                    supported_claims = grounding_report.supported_claims,
                    unsupported_claims = grounding_report.unsupported_count(),
                    grounded_ratio = grounding_report.grounded_ratio(),
                    "Response grounding verification complete"
                );
            }
            let verification_metadata = if verification_enabled {
                Some(serde_json::json!({
                    "enabled": true,
                    "claimsEvaluated": grounding_report.claims_evaluated,
                    "supportedClaims": grounding_report.supported_claims,
                    "supportedClaimNotes": grounding_report.supported_claim_notes,
                    "unsupportedClaims": grounding_report.unsupported_claims,
                    "groundedRatio": grounding_report.grounded_ratio(),
                }))
            } else {
                Some(serde_json::json!({
                    "enabled": false
                }))
            };
            flow_metrics.verification_ms = elapsed_ms(verification_start);

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

            // Return original error
            Err(e)
        }
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn retrieval_subtimings_or_default(
    flow_metrics: &ConversationFlowTimingMetrics,
) -> RetrievalSubTimingMetrics {
    flow_metrics
        .retrieval_subtimings
        .clone()
        .unwrap_or_else(RetrievalSubTimingMetrics::default)
}

fn generation_subtimings_or_default(
    flow_metrics: &ConversationFlowTimingMetrics,
) -> ToolLoopTimingMetrics {
    flow_metrics
        .generation_subtimings
        .clone()
        .unwrap_or_else(ToolLoopTimingMetrics::default)
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

    prompt_settings.system_prompt = enforce_numeric_citation_format(&prompt_settings.system_prompt);
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

const LINKED_WEB_SOURCE_PROMPT_MAX_ITEMS: i64 = 10;
const LINKED_WEB_SOURCE_PROMPT_MAX_EXCERPT_CHARS: usize = 280;
const LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_RATIO: f64 = 0.08;
const LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_MIN: usize = 120;
const LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_MAX: usize = 320;

#[derive(Debug, sqlx::FromRow)]
struct ConversationWebSourcePromptRow {
    title: Option<String>,
    url: String,
    excerpt: Option<String>,
}

async fn build_linked_web_sources_prompt_context(
    container: &Container,
    conversation_id: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    max_tokens: usize,
) -> Result<Option<String>> {
    let rows = sqlx::query_as::<_, ConversationWebSourcePromptRow>(
        r#"
        SELECT title, url, excerpt
        FROM conversation_web_sources
        WHERE conversation_id = ?
        ORDER BY added_at DESC
        LIMIT ?
        "#,
    )
    .bind(conversation_id)
    .bind(LINKED_WEB_SOURCE_PROMPT_MAX_ITEMS)
    .fetch_all(container.db_pool())
    .await
    .map_err(|error| {
        AppError::Database(format!(
            "Failed to load linked web sources for conversation context: {}",
            error
        ))
    })?;

    if rows.is_empty() {
        return Ok(None);
    }

    let raw_budget =
        ((max_tokens as f64) * LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_RATIO).round() as usize;
    let token_budget = raw_budget.clamp(
        LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_MIN,
        LINKED_WEB_SOURCE_PROMPT_TOKEN_BUDGET_MAX,
    );

    let mut entries: Vec<String> = Vec::new();
    let mut used_tokens = 0usize;

    for row in rows {
        let url = row.url.trim();
        if url.is_empty() {
            continue;
        }

        let title = row
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(url);

        let mut entry = format!("- {} ({})", title, url);
        if let Some(excerpt) = row
            .excerpt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let truncated_excerpt =
                safe_truncate(excerpt, LINKED_WEB_SOURCE_PROMPT_MAX_EXCERPT_CHARS);
            entry.push_str(&format!("\n  Excerpt: {}", truncated_excerpt));
        }

        let entry_tokens = llm.count_tokens(&entry);
        if !entries.is_empty() && used_tokens.saturating_add(entry_tokens) > token_budget {
            break;
        }

        used_tokens = used_tokens.saturating_add(entry_tokens);
        entries.push(entry);
    }

    if entries.is_empty() {
        return Ok(None);
    }

    Ok(Some(format!(
        "User-linked web sources for this conversation (context links, not necessarily indexed):\n{}",
        entries.join("\n")
    )))
}

async fn build_conversation_context(
    container: &Container,
    conv_service: &Arc<dyn crate::infrastructure::services::traits::ConversationServiceTrait>,
    conversation_id: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    max_tokens: usize,
    global_system_prompt: &str,
) -> Result<(
    Vec<String>,
    Vec<crate::domain::conversation::DocumentReference>,
    Option<String>,
)> {
    use crate::infrastructure::services::ConversationService as ConcreteConversationService;

    let concrete_service = Arc::new(ConcreteConversationService::new(
        container.db_pool().clone(),
    ));
    let token_counter = {
        let llm = Arc::clone(llm);
        Arc::new(move |text: &str| llm.count_tokens(text))
    };

    let context_builder = ContextWindowBuilder::new(concrete_service, max_tokens, token_counter)
        .with_recent_message_count(8);

    let conversation_aggregate = conv_service.get_conversation(conversation_id).await?;
    let conversation_system_prompt = conversation_aggregate
        .as_ref()
        .and_then(|aggregate| aggregate.system_prompt().map(|s| s.to_string()))
        .and_then(|prompt| {
            let trimmed = prompt.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
    let conversation_document_context = conversation_aggregate
        .as_ref()
        .map(|aggregate| aggregate.document_context().to_vec())
        .unwrap_or_default();
    let space_system_prompt: Option<String> = sqlx::query_scalar(
        r#"
        SELECT s.space_prompt
        FROM conversations c
        LEFT JOIN conversation_spaces s ON s.id = c.space_id
        WHERE c.id = ?
        LIMIT 1
        "#,
    )
    .bind(conversation_id)
    .fetch_optional(container.db_pool())
    .await
    .map_err(|e| AppError::Database(format!("Failed to load space prompt: {}", e)))?
    .flatten()
    .and_then(|prompt: String| {
        let trimmed = prompt.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let mut context = context_builder.build(conversation_id).await?;
    let global_prompt = global_system_prompt.trim();
    let effective_system_prompt = conversation_system_prompt
        .or(space_system_prompt)
        .or_else(|| (!global_prompt.is_empty()).then(|| global_prompt.to_string()));
    if let Some(system_prompt) = effective_system_prompt {
        let entry = format!("System: {}", system_prompt);
        if !context
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&entry))
        {
            context.insert(0, entry);
        }
    }

    let linked_web_sources_context =
        build_linked_web_sources_prompt_context(container, conversation_id, llm, max_tokens)
            .await?;

    Ok((
        context,
        conversation_document_context,
        linked_web_sources_context,
    ))
}

async fn trigger_background_summary_refresh_if_needed(
    container: &Container,
    conv_service: &Arc<dyn crate::infrastructure::services::traits::ConversationServiceTrait>,
    conversation_id: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    max_tokens: usize,
) {
    const SUMMARY_TRIGGER_RATIO: f64 = 0.72;
    const ESTIMATED_RESPONSE_RATIO: f64 = 0.25;
    const MIN_COMPLETED_MESSAGES: usize = 10;

    let aggregate = match conv_service.get_conversation(conversation_id).await {
        Ok(Some(aggregate)) => aggregate,
        Ok(None) => return,
        Err(e) => {
            warn!(
                conversation_id = conversation_id,
                error = %e,
                "Failed loading conversation for summary scheduling"
            );
            return;
        }
    };

    let completed_messages: Vec<_> = aggregate
        .messages()
        .iter()
        .filter(|m| m.is_completed())
        .cloned()
        .collect();
    if completed_messages.len() < MIN_COMPLETED_MESSAGES {
        return;
    }

    let transcript_tokens: usize = completed_messages
        .iter()
        .map(|m| llm.count_tokens(&format!("{}: {}", m.role, m.content)))
        .sum();
    let projected_total_tokens =
        transcript_tokens + (max_tokens as f64 * ESTIMATED_RESPONSE_RATIO) as usize;
    let trigger_threshold_tokens = (max_tokens as f64 * SUMMARY_TRIGGER_RATIO) as usize;

    if projected_total_tokens < trigger_threshold_tokens {
        return;
    }

    let event = ConversationEvent::summary_refresh_requested(
        conversation_id.to_string(),
        completed_messages,
        transcript_tokens,
        Arc::clone(llm),
    );
    if let Err(e) = container.conversation_event_bus().publish(event) {
        warn!(
            conversation_id = conversation_id,
            error = %e,
            "Failed to publish conversation summary refresh event"
        );
        return;
    }

    info!(
        conversation_id = conversation_id,
        projected_total_tokens = projected_total_tokens,
        threshold_tokens = trigger_threshold_tokens,
        "Published conversation summary refresh event"
    );
}

async fn resolve_router_decision(
    container: &Container,
    conversation_document_context: &[crate::domain::conversation::DocumentReference],
    validated_message: &str,
    force_web_search: bool,
    router_settings: &RouterSettingsDto,
) -> Result<RouterDecisionOutcome> {
    if force_web_search {
        return Ok(RouterDecisionOutcome {
            action: RouterAction::NewSearch,
            recent_doc_meta: None,
            clarify_message: None,
        });
    }

    let recent_doc_meta =
        load_recent_document_metadata(container, conversation_document_context).await;
    let has_recent_document = recent_doc_meta.is_some();
    if conversation_document_context.is_empty() || !router_settings.enabled || !has_recent_document
    {
        return Ok(RouterDecisionOutcome {
            action: RouterAction::NewSearch,
            recent_doc_meta,
            clarify_message: None,
        });
    }

    let router_input = RouterInput {
        query: validated_message.to_string(),
        has_recent_document,
        recent_document_title: recent_doc_meta.as_ref().map(|m| m.title.clone()),
        recent_document_id: recent_doc_meta.as_ref().map(|m| m.document_id.clone()),
    };
    let router_llm = container.get_or_load_router_llm().await?;
    let router = RouterService::new(router_llm, router_settings.clone());
    let decision = router.route(router_input).await;

    Ok(RouterDecisionOutcome {
        action: decision.action,
        recent_doc_meta,
        clarify_message: decision.clarify_question,
    })
}

fn budget_search_results_for_prompt<'a>(
    search_results: &'a [crate::application::dtos::search_dto::SearchResultDto],
    available_for_rag: usize,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
) -> Vec<&'a crate::application::dtos::search_dto::SearchResultDto> {
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

    let optional_allowlist = tool_preferences
        .and_then(|preferences| preferences.enabled_tools.as_ref())
        .map(|enabled_tools| {
            enabled_tools
                .iter()
                .map(|name| name.trim())
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .collect::<HashSet<String>>()
        });

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
                    .map_or(true, |allowlist| allowlist.contains(tool_name));
            }

            if !enabled_custom_tools.contains_key(tool_name) {
                // Skip stale custom tool definitions that are no longer configured.
                return false;
            }

            optional_allowlist
                .as_ref()
                .map_or(true, |allowlist| allowlist.contains(tool_name))
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
                    .map_or(true, |allowlist| allowlist.contains(&tool.name))
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

fn generate_title(message: &str) -> String {
    const MAX_LEN: usize = 50;

    if message.len() <= MAX_LEN {
        message.to_string()
    } else {
        format!("{}...", &message[..MAX_LEN])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::dtos::search_dto::SearchResultDto;
    use std::collections::HashMap;

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
        // Should be exactly 50 chars from original + "..."
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
        };

        let flags = SearchFlags::from_preferences(Some(&prefs));
        assert!(!flags.force_kb_search);
        assert!(!flags.force_web_search);
        assert!(flags.force_followup_mode);
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
        };

        let flags = SearchFlags::from_preferences(Some(&prefs));
        assert!(flags.force_kb_search);
        assert!(flags.force_web_search);
        assert!(!flags.force_followup_mode);
    }

    fn make_result(id: &str, doc_id: &str, score: f32) -> SearchResultDto {
        SearchResultDto {
            id: id.to_string(),
            title: id.to_string(),
            content: format!("chunk {} about search relevance", id),
            score,
            path: None,
            document_id: Some(doc_id.to_string()),
            position: None,
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata: HashMap::new(),
        }
    }

    fn make_custom_result(
        id: &str,
        doc_id: &str,
        title: &str,
        content: &str,
        score: f32,
    ) -> SearchResultDto {
        SearchResultDto {
            id: id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            score,
            path: None,
            document_id: Some(doc_id.to_string()),
            position: None,
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_keyword_plan_enriches_acronym_query_with_hyde_terms() {
        let plan = retrieval::build_keyword_query_plan(
            "What is happening with NASA right now?",
            Some(
                "NASA reports new missions from the national aeronautics and space administration.",
            ),
        );

        assert!(!plan.terms.is_empty());
        assert!(plan.terms.iter().any(|term| term == "nasa"));
        assert!(plan.terms.iter().any(|term| [
            "national",
            "aeronautics",
            "space",
            "administration"
        ]
        .contains(&term.as_str())));
        assert!(plan.query.contains(" OR "));
    }

    #[test]
    fn test_overlap_query_terms_drop_modifier_words() {
        let terms = retrieval::extract_overlap_query_terms(
            "What is specifically going on with NASA right now?",
        );
        assert_eq!(terms, vec!["nasa".to_string()]);
    }

    #[test]
    fn test_document_support_filter_drops_weak_singleton_outlier_doc() {
        let input = vec![
            make_result("a1", "doc-a", 0.0131),
            make_result("a2", "doc-a", 0.0129),
            make_result("a3", "doc-a", 0.0127),
            make_result("b1", "doc-b", 0.0125),
            make_result("a4", "doc-a", 0.0123),
        ];

        let filtered = retrieval::filter_results_by_document_support(input);
        assert_eq!(filtered.len(), 4);
        assert!(filtered
            .iter()
            .all(|result| result.document_id.as_deref() == Some("doc-a")));
    }

    #[test]
    fn test_document_support_filter_keeps_close_competitor_docs() {
        let input = vec![
            make_result("a1", "doc-a", 0.40),
            make_result("b1", "doc-b", 0.35),
        ];

        let filtered = retrieval::filter_results_by_document_support(input);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_overlap_filter_acronym_query_keeps_context_supported_results() {
        let input = vec![
            make_custom_result(
                "nasa-1",
                "doc-nasa-1",
                "NASA mission operations",
                "Recent updates on NASA launch activities",
                0.90,
            ),
            make_custom_result(
                "nasa-2",
                "doc-nasa-2",
                "Space agency mission briefing",
                "National aeronautics administration mission status update",
                0.88,
            ),
            make_custom_result(
                "off-1",
                "doc-off-1",
                "Campus policy dispute",
                "Statement about student organization and local politics",
                0.86,
            ),
        ];

        let filtered = retrieval::filter_results_by_query_overlap(
            input,
            "What is specifically going on with NASA right now?",
            Some("NASA mission updates from the national aeronautics and space administration."),
            None,
        );
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|result| result.id == "nasa-1"));
        assert!(filtered.iter().any(|result| result.id == "nasa-2"));
    }

    #[test]
    fn test_overlap_filter_acronym_query_returns_empty_when_no_support() {
        let input = vec![
            make_custom_result(
                "off-1",
                "doc-off-1",
                "University club political controversy",
                "Statement about campus politics and student groups",
                0.92,
            ),
            make_custom_result(
                "off-2",
                "doc-off-2",
                "Congressional hearing drama",
                "Public testimony and committee conflict details",
                0.89,
            ),
        ];

        let filtered = retrieval::filter_results_by_query_overlap(
            input,
            "What has been going on with NASA?",
            None,
            None,
        );
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_overlap_filter_short_non_acronym_query_keeps_fallback() {
        let input = vec![
            make_custom_result(
                "off-1",
                "doc-off-1",
                "University club political controversy",
                "Statement about campus politics and student groups",
                0.92,
            ),
            make_custom_result(
                "off-2",
                "doc-off-2",
                "Congressional hearing drama",
                "Public testimony and committee conflict details",
                0.89,
            ),
        ];

        let filtered =
            retrieval::filter_results_by_query_overlap(input, "policy update", None, None);
        assert_eq!(filtered.len(), 2);
    }
}
