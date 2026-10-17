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
//! - **Security**: Input validation
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

use self::intent::{IntentClassifier, TurnIntent};
use self::router::{RouterAction, RouterInput, RouterService};
use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::conversation::dto::CreateConversationRequestDto;
use lattice_core::application::contracts::settings::{CustomToolSettingsDto, RouterSettingsDto};
use lattice_core::application::services::conversation_context::build_conversation_context;
use lattice_core::domain::qa::hyde::QueryType;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::extract_highlight_terms;
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
// Crate-visible so the composition root can own the one turn-cancellation
// registry and hand it to every turn through the chat runtime.
pub(crate) mod cancellation;
mod document_text;
mod fetch_memory;
mod focus;
// Public so the shipped-path evaluation can drive the desktop's own turn.
pub mod headless;
mod prior_evidence;
// Public so the no-tools retrieval path and the turn's tool-selection wiring can
// reach it without this module re-exporting its whole surface.
pub mod history_tools;
pub mod intent;
pub mod memory_context;
mod persistence;
pub(crate) use persistence::index_memory_note;
mod prompting;
pub(crate) mod source_snapshots;
// Public so the retrieval evaluation harness can reuse the pipeline's own
// sufficiency judgement instead of reimplementing it.
mod research_job;
pub mod retrieval;
pub mod router;
pub use research_job::{
    job_config as research_job_config, resumable_request_ids as resumable_research_requests,
    DeepResearchJob, ResearchWaiters, DEEP_RESEARCH,
};
mod tool_loop;
pub mod turn_record;
// Public so the claim-checker evaluation harness measures the verifier that
// ships instead of a copy of its rules.
pub mod verification;
mod web_steps;

use self::attachments::{build_turn_attachments, TurnAttachments};

/// How much of an attachment the web-query rewriter is shown. Enough to name
/// the subject, small enough that the utility model stays fast.
const ATTACHMENT_DIGEST_CHARS: usize = 900;
use self::cancellation::TurnCancellationRegistry;
use self::focus::FocusScope;
use self::persistence::{
    finalize_successful_turn, mark_user_message_failed, persist_user_message_pending,
    FinishedAnswer,
};
use self::prompting::PromptMessageBuilder;
pub use self::retrieval::RetrievalSubTimingMetrics;
use self::retrieval::{
    assign_citation_ids, citation_ids_by_chunk, confine_document_context, deduplicate_sources,
    load_recent_document_metadata, run_retrieval_pipeline, RouterDecisionOutcome,
};
pub use self::source_snapshots::CapturedSourcesDto;
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

struct TurnCancellationGuard {
    request_id: String,
    cancellation: Arc<TurnCancellationRegistry>,
}

impl TurnCancellationGuard {
    /// `parent` is the stop of the job running the turn, if one is.
    fn start(
        cancellation: Arc<TurnCancellationRegistry>,
        request_id: String,
        conversation_id: &str,
        parent: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<Self> {
        if !cancellation.begin_turn_within(&request_id, conversation_id, parent) {
            return Err(AppError::InvalidState(
                "A generation with this request ID is already in flight.".to_string(),
            ));
        }
        Ok(Self {
            request_id,
            cancellation,
        })
    }

    /// The token a stop press for this turn fires, handed to every model
    /// request it makes.
    fn turn_token(&self) -> tokio_util::sync::CancellationToken {
        self.cancellation.turn_token(&self.request_id)
    }

    /// Whether a stop press has fired for this turn.
    fn is_cancel_requested(&self) -> bool {
        self.cancellation.is_cancel_requested(&self.request_id)
    }
}

impl Drop for TurnCancellationGuard {
    fn drop(&mut self) {
        self.cancellation.finish_turn(&self.request_id);
    }
}

/// One turn as retrieval, the tool loop and persistence read it: the
/// conversation and request it belongs to, the question, the model answering
/// it, and the focus and recorder every search and tool call goes through.
#[derive(Clone, Copy)]
struct ChatTurn<'a> {
    conv_service: &'a Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &'a str,
    request_id: &'a str,
    /// The user's message, validated.
    message: &'a str,
    llm: &'a Arc<dyn lattice_core::application::ports::LLMPort>,
    highlight_terms: &'a [String],
    focus: &'a FocusScope,
    recorder: &'a TurnRecorder,
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
    llm: &Arc<dyn lattice_core::application::ports::LLMPort>,
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
            lattice_core::shared::text::safe_truncate(message, MAX_CHARS)
        )
    }
}

#[cfg(test)]
mod test_runtime;
#[cfg(test)]
mod tests;
