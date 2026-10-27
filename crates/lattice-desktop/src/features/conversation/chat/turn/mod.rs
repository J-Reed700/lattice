//! One chat turn, as a sequence of stages with typed hand-offs:
//!
//! 1. [`prepare`]: validate, open the conversation, pin its model, read its
//!    history and settings, and plan the turn's budget.
//! 2. [`classify`]: what the turn needs (intent), then where to look (router).
//! 3. [`carry`]: the material the turn reads whole — attachments, the folder.
//! 4. [`retrieve`]: evidence earlier turns cited, then the retrieval pipeline.
//! 5. [`assemble`]: fit evidence to the budget, number it, render the prompt,
//!    and plan the typed request, which may cut the lowest-ranked evidence
//!    when the window cannot hold all of it.
//! 6. [`generate`]: the agentic tool loop.
//! 7. [`finalize`]: persist the answer, then start the background check.
//!
//! A deep research turn runs the same stages inside a job
//! ([`super::research_job`]); [`research`] is what it saves on the way, and a
//! resumed one starts at stage 6 from there.
//!
//! Every pool the prompt carries is sized from one
//! [`BudgetAllocation`](lattice_core::application::services::context_assembler::BudgetAllocation):
//! the turn's [`budget::TurnBudget`] before retrieval, and the context
//! assembler's plan when the request is assembled.
use super::dto::SourceDto;
use super::*;
use lattice_core::domain::models::model_ref::ModelRef;

mod assemble;
mod budget;
mod carry;
mod classify;
mod finalize;
mod generate;
mod message;
mod pictures;
mod prepare;
mod research;
mod retrieve;

pub(super) use research::{AssembledTurn, Research, ResearchJournal, ResearchRound, Rounds, Saved};

#[cfg(test)]
mod pin_tests;
#[cfg(test)]
mod tests;

/// What the reader attached to a message: the files by name, the library
/// documents they became, and the pictures staged for the model to see.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageAttachments {
    pub names: Vec<String>,
    pub document_ids: Vec<String>,
    pub image_ids: Vec<String>,
}

/// What the caller asked of a turn.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TurnRequest {
    pub(super) conversation_id: Option<String>,
    pub(super) message: String,
    pub(super) tool_preferences: Option<ToolPreferences>,
    pub(super) request_id: Option<String>,
    pub(super) attachment_names: Vec<String>,
    pub(super) attachment_document_ids: Vec<String>,
    /// Pictures the reader attached, by `chat_images` id. Their pixels ride
    /// in this turn's request only.
    #[serde(default)]
    pub(super) attachment_image_ids: Vec<String>,
    /// A model for this request only, over the chat's own choice. Never
    /// persisted on the chat; a research turn carries it in its job so a
    /// resumed turn answers with the same model.
    pub(super) model: Option<ModelRef>,
}

/// Runs one chat turn, creating the conversation when none is named. A deep
/// research turn runs as a job and this waits for its answer.
///
/// # Errors
///
/// * `AppError::NoActiveModel` - No chat model is active
/// * `AppError::InvalidInput` - Empty or invalid message, or a message the
///   model's window cannot hold
/// * `AppError::Database` - Database operation failed
pub(super) async fn run_turn(
    container: &dyn ChatRuntime,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    request_id: Option<String>,
    attachments: MessageAttachments,
    model: Option<ModelRef>,
    emit: ChatEventSink,
) -> Result<ChatResponse> {
    if cancel_only.unwrap_or(false) {
        let conv_id = conversation_id.ok_or_else(|| {
            AppError::InvalidInput("Conversation ID is required to cancel generation".to_string())
        })?;
        // A research turn is a job: the stop cancels the job as well as the
        // turn, so it ends rather than resuming at the next start.
        let research = super::research_job::stop(container, &conv_id, request_id.as_deref())
            .await
            .unwrap_or_else(|error| {
                warn!(%error, conversation_id = conv_id.as_str(), "Could not stop a research job");
                false
            });
        let cancelled = container
            .turn_cancellation()
            .request_cancel(&conv_id, request_id.as_deref())
            || research;
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

    let request = TurnRequest {
        conversation_id,
        message,
        tool_preferences,
        request_id,
        attachment_names: attachments.names,
        attachment_document_ids: attachments.document_ids,
        attachment_image_ids: attachments.image_ids,
        model,
    };
    if SearchFlags::from_preferences(request.tool_preferences.as_ref()).deep_research_mode {
        return super::research_job::run(container, request, emit).await;
    }
    run_stages(container, request, &emit, None).await
}

/// The stages in order. A research turn passes its [`Research`]: the turn
/// saves itself through it as it goes, and one saved before a restart carries
/// on from there.
pub(super) async fn run_stages(
    container: &dyn ChatRuntime,
    request: TurnRequest,
    emit: &ChatEventSink,
    research: Option<Research<'_>>,
) -> Result<ChatResponse> {
    tracing::info!(
        conversation_id = request.conversation_id.as_deref().unwrap_or("NEW"),
        content_length = request.message.len(),
        resumed = research
            .as_ref()
            .is_some_and(|research| research.saved.is_some()),
        "chat_with_conversation: START"
    );
    let mut metrics = ConversationFlowTimingMetrics::default();
    let (journal, parent, saved) = match research {
        Some(Research {
            journal,
            cancel,
            saved,
        }) => (Some(journal), Some(cancel), saved),
        None => (None, None, None),
    };
    if let (Some(journal), Some(parent), Some(saved)) = (journal, parent.as_ref(), saved) {
        return resume_stages(container, &request, journal, parent, saved, emit, metrics).await;
    }

    let flow_start = Instant::now();
    let preferences = request.tool_preferences.as_ref();
    let turn = prepare::prepare_turn(
        container,
        request.conversation_id.clone(),
        &request.message,
        request.request_id.clone(),
        request.model.as_ref(),
        &request.attachment_image_ids,
        preferences,
        parent.as_ref(),
        emit,
        &mut metrics,
    )
    .await?;
    let flags = classify::classify_turn(container, &turn, preferences).await;
    let mut evidence = turn.budget.evidence;
    let carried = carry::carry_material(
        container,
        &turn,
        flags,
        request.attachment_names.clone(),
        request.attachment_document_ids.clone(),
        &mut evidence,
    )
    .await;
    let route = classify::route_turn(container, &turn, flags, &mut metrics).await?;
    turn.ensure_not_cancelled()?;
    let retrieved = retrieve::retrieve_evidence(
        container,
        &turn,
        flags,
        &route,
        &carried,
        evidence,
        &mut metrics,
    )
    .await?;
    turn.ensure_not_cancelled()?;

    let mut prompt = assemble::render_prompt(
        &turn,
        flags,
        preferences,
        carried,
        retrieved,
        emit,
        &mut metrics,
    );
    let Some(journal) = journal else {
        let persist_start = Instant::now();
        let (user_message_id, message_tokens) = persist_question(container, &turn, &prompt).await?;
        metrics.persist_user_message_ms = elapsed_ms(persist_start);
        // From here a failure leaves the saved question retryable rather than
        // pending forever, so every error goes through the same cleanup.
        let generated = match assemble::plan_request(
            container,
            &turn,
            flags,
            preferences,
            &mut prompt,
            &mut metrics,
        )
        .await
        {
            Ok(request) => {
                generate::generate_answer(
                    container,
                    &turn,
                    flags,
                    prompt,
                    request,
                    None,
                    emit,
                    &mut metrics,
                )
                .await
            }
            Err(error) => Err(error),
        };
        let answered = Answered {
            flags,
            router: route.record,
            user_message_id,
            message_tokens,
            flow_start,
        };
        return conclude(container, &turn, None, answered, generated, emit, metrics).await;
    };

    // A research turn plans before it saves the question, then saves both at
    // once: a restart between the two would otherwise ask the question twice.
    let planned = assemble::plan_request(
        container,
        &turn,
        flags,
        preferences,
        &mut prompt,
        &mut metrics,
    )
    .await?;
    let persist_start = Instant::now();
    let (user_message_id, message_tokens) = persist_question(container, &turn, &prompt).await?;
    metrics.persist_user_message_ms = elapsed_ms(persist_start);
    let assembled = AssembledTurn {
        user_message_id: user_message_id.clone(),
        message_tokens,
        history_len: turn.history_len,
        flags,
        router: route.record.clone(),
        input: planned.input.clone(),
        tools: planned.tools.clone(),
        max_output_tokens: planned.max_output_tokens,
        input_budget: planned.input_budget,
        memory_usage: planned.memory_usage.clone(),
        short_circuit_response: prompt.short_circuit_response.clone(),
        sources: prompt.sources.clone(),
        retrieval_trace: prompt.retrieval_trace.clone(),
        pages_read: prompt.pages_read.clone(),
        steps: turn.recorder.steps(),
        elapsed_ms: turn.recorder.elapsed_ms(),
    };
    let generated = match journal.assembled(&assembled).await {
        Ok(()) => {
            generate::generate_answer(
                container,
                &turn,
                flags,
                prompt,
                planned,
                Some(research::Rounds {
                    journal,
                    start: None,
                }),
                emit,
                &mut metrics,
            )
            .await
        }
        Err(error) => Err(error),
    };
    let answered = Answered {
        flags,
        router: route.record,
        user_message_id,
        message_tokens,
        flow_start,
    };
    conclude(
        container,
        &turn,
        Some(journal),
        answered,
        generated,
        emit,
        metrics,
    )
    .await
}

/// A research turn saved before a restart, carried on from its last round.
async fn resume_stages(
    container: &dyn ChatRuntime,
    request: &TurnRequest,
    journal: &dyn ResearchJournal,
    parent: &tokio_util::sync::CancellationToken,
    saved: Saved,
    emit: &ChatEventSink,
    mut metrics: ConversationFlowTimingMetrics,
) -> Result<ChatResponse> {
    let flow_start = Instant::now()
        .checked_sub(Duration::from_millis(saved.elapsed_ms()))
        .unwrap_or_else(Instant::now);
    let turn = prepare::resume_turn(container, request, &saved, parent, emit).await?;
    let Saved { assembled, round } = saved;
    let AssembledTurn {
        user_message_id,
        message_tokens,
        flags,
        router,
        input,
        tools,
        max_output_tokens,
        input_budget,
        memory_usage,
        short_circuit_response,
        sources,
        retrieval_trace,
        pages_read,
        ..
    } = assembled;
    let (sources, retrieval_trace, pages_read, start) = match round {
        Some(round) => (
            round.sources.clone(),
            round.retrieval_trace.clone(),
            round.pages_read.clone(),
            Some(research::LoopStart::from(round)),
        ),
        None => (sources, retrieval_trace, pages_read, None),
    };
    info!(
        conversation_id = turn.conv_id.as_str(),
        rounds = start.as_ref().map_or(0, |start| start.rounds),
        "Resuming a research turn"
    );
    let prompt = assemble::RenderedPrompt::resumed(
        sources,
        retrieval_trace,
        short_circuit_response,
        pages_read,
    );
    let planned = assemble::PlannedRequest {
        input,
        tools,
        max_output_tokens,
        input_budget,
        memory_usage,
        // Not saved with the turn. If the window still has to move, the next
        // turn's plan asks again.
        summary_refold: None,
    };
    let generated = generate::generate_answer(
        container,
        &turn,
        flags,
        prompt,
        planned,
        Some(research::Rounds { journal, start }),
        emit,
        &mut metrics,
    )
    .await;
    let answered = Answered {
        flags,
        router,
        user_message_id,
        message_tokens,
        flow_start,
    };
    conclude(
        container,
        &turn,
        Some(journal),
        answered,
        generated,
        emit,
        metrics,
    )
    .await
}

/// The question saved pending on the thread, and what it costs in tokens.
async fn persist_question(
    container: &dyn ChatRuntime,
    turn: &prepare::PreparedTurn,
    prompt: &assemble::RenderedPrompt,
) -> Result<(String, usize)> {
    persist_user_message_pending(
        container,
        &turn.conv_service,
        &turn.conv_id,
        &turn.turn_id,
        &turn.message,
        super::persistence::QuestionAttachments {
            names: &prompt.attachment_names,
            document_ids: &prompt.attachment_ids,
            image_ids: &turn.picture_ids(),
            pictures_seen: prompt.pictures.seen,
        },
        &turn.llm,
    )
    .await
}

/// What a generated answer is saved with.
struct Answered {
    flags: SearchFlags,
    router: Option<TurnRouterDto>,
    user_message_id: String,
    message_tokens: usize,
    flow_start: Instant,
}

/// Saves the answer, or marks the question failed. A research turn stopping
/// to resume later leaves its question pending.
async fn conclude(
    container: &dyn ChatRuntime,
    turn: &prepare::PreparedTurn,
    journal: Option<&dyn ResearchJournal>,
    answered: Answered,
    generated: Result<generate::GeneratedAnswer>,
    emit: &ChatEventSink,
    mut metrics: ConversationFlowTimingMetrics,
) -> Result<ChatResponse> {
    match generated {
        Ok(generated) => {
            finalize::finalize_turn(
                container,
                turn,
                answered.flags,
                generated,
                answered.router,
                &answered.user_message_id,
                answered.message_tokens,
                emit,
                answered.flow_start,
                metrics,
            )
            .await
        }
        Err(error) => {
            let resumes = match journal {
                Some(journal) => journal.resumes_later().await,
                None => false,
            };
            if resumes {
                info!(
                    conversation_id = turn.conv_id.as_str(),
                    "Research turn stopped to resume at the next start"
                );
                return Err(error);
            }
            tracing::error!(
                conversation_id = turn.conv_id.as_str(),
                error = %error,
                "chat_with_conversation: LLM generation FAILED"
            );
            mark_user_message_failed(&turn.conv_service, &answered.user_message_id).await;
            super::persistence::release_turn_images(container, &turn.turn_id).await;
            metrics.total_ms = elapsed_ms(answered.flow_start);
            finalize::log_flow_timing(&turn.conv_id, &metrics, true);
            Err(error)
        }
    }
}
