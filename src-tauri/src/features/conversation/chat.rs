//! Conversation-based Chat Command
//!
//! Provides chat functionality with conversation history and context window management.
//! Desktop transport, wire contracts, routing policy, and turn orchestration live
//! in separate modules; the public command and DTO paths are preserved.
//!
//! # Features
//!
//! - **Conversation History**: Maintains persistent chat history
//! - **Context Window Management**: one budget from the context assembler for
//!   the prompt's every pool and the generation reservation
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
use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::conversation::dto::CreateConversationRequestDto;
use crate::features::settings::dto::{CustomToolSettingsDto, RouterSettingsDto};
use crate::infrastructure::services::intent::{IntentClassifier, IntentInput, TurnIntent};
use crate::infrastructure::services::router::{RouterAction, RouterInput, RouterService};
use crate::shared::error::{AppError, Result};
use crate::shared::text::extract_highlight_terms;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

mod desktop;
pub mod ports;
pub use desktop::{app_event_sink, chat_with_conversation_impl};

/// Transport-independent delivery of a typed chat event.
pub type ChatEventSink = Arc<dyn Fn(ChatStreamEventDto) -> Result<()> + Send + Sync>;

mod attachments;
mod cancellation;
mod document_text;
mod fetch_memory;
mod focus;
mod prior_evidence;
// Public so the no-tools retrieval path and the turn's tool-selection wiring can
// reach it without this module re-exporting its whole surface.
pub mod history_tools;
pub mod memory_context;
mod persistence;
pub(crate) use persistence::index_memory_note;
mod prompting;
pub(crate) mod source_snapshots;
// Public so the retrieval evaluation harness can reuse the pipeline's own
// sufficiency judgement instead of reimplementing it.
mod research_job;
pub mod retrieval;
pub use research_job::{job_config as research_job_config, DeepResearchJob, DEEP_RESEARCH};
mod tool_loop;
pub mod turn_record;
mod verification;
mod web_steps;

use self::attachments::{build_turn_attachments, TurnAttachments};

/// How much of an attachment the web-query rewriter is shown. Enough to name
/// the subject, small enough that the utility model stays fast.
const ATTACHMENT_DIGEST_CHARS: usize = 900;
use self::cancellation::{begin_turn_within, finish_turn, is_cancel_requested};
use self::focus::FocusScope;
use self::persistence::{
    finalize_successful_turn, mark_user_message_failed, persist_user_message_pending,
};
use self::prompting::{build_kb_context, PromptMessageBuilder};
pub use self::retrieval::RetrievalSubTimingMetrics;
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
pub use self::verification::VerificationReadyDto;
use self::verification::{pending_metadata, BackgroundVerification};
pub(crate) use crate::features::explorer::prompt::ExplorerTurn;

pub fn cancel_generation_for_conversation(conversation_id: &str, request_id: Option<&str>) -> bool {
    cancellation::request_cancel(conversation_id, request_id)
}

struct TurnCancellationGuard {
    request_id: String,
}

impl TurnCancellationGuard {
    /// `parent` is the stop of the job running the turn, if one is.
    fn start(
        request_id: String,
        conversation_id: &str,
        parent: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<Self> {
        if !begin_turn_within(&request_id, conversation_id, parent) {
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

mod dto;
mod prompt_settings;
mod routing;
mod tools;
mod turn;

use dto::{trace_counts, TraceCounts};
pub use dto::{
    ChatResponse, ChatStreamEventDto, ConversationFlowTimingMetrics, ConversationMessage,
    RetrievalTraceDto, ToolPreferences,
};
use prompt_settings::*;
use routing::*;
use tools::*;
use turn::run_turn;

/// Runs a turn no window watches, such as a part of a journal synthesis: its
/// stream goes nowhere and only its answer is used.
pub(crate) async fn run_unwatched_turn(
    container: &dyn ChatRuntime,
    conversation_id: String,
    message: String,
    tool_preferences: ToolPreferences,
) -> Result<ChatResponse> {
    run_turn(
        container,
        Some(conversation_id),
        message,
        Some(tool_preferences),
        None,
        None,
        None,
        None,
        Arc::new(|_| Ok(())),
    )
    .await
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
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

async fn validate_and_guard_chat_request(
    runtime: &dyn ChatRuntime,
    message: &str,
) -> Result<String> {
    runtime.validate_message(message).await
}

async fn get_or_create_conversation_id(
    container: &dyn ChatRuntime,
    conversation_id: Option<String>,
    validated_message: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
) -> Result<String> {
    match conversation_id {
        Some(id) => Ok(id),
        None => {
            let request = CreateConversationRequestDto {
                title: generate_title(validated_message),
                model_name: llm.model_name().to_string(),
                system_prompt: None,
            };
            let response = container.create_conversation(request).await?;
            Ok(response.conversation.id)
        }
    }
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
            crate::shared::text::safe_truncate(message, MAX_CHARS)
        )
    }
}

#[cfg(test)]
mod test_runtime;
#[cfg(test)]
mod tests;
