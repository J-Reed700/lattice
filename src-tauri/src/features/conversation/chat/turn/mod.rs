//! One chat turn, as a sequence of stages with typed hand-offs:
//!
//! 1. [`prepare`]: validate, load the model, open the conversation, read its
//!    history and settings, and plan the turn's budget.
//! 2. [`classify`]: what the turn needs (intent), then where to look (router).
//! 3. [`carry`]: the material the turn reads whole — attachments, the folder.
//! 4. [`retrieve`]: evidence earlier turns cited, then the retrieval pipeline.
//! 5. [`assemble`]: fit evidence to the budget, number it, render the prompt,
//!    and plan the typed request.
//! 6. [`generate`]: the agentic tool loop.
//! 7. [`finalize`]: persist the answer, then start the background check.
//!
//! Every pool the prompt carries is sized from one
//! [`BudgetAllocation`](crate::application::services::context_assembler::BudgetAllocation):
//! the turn's [`budget::TurnBudget`] before retrieval, and the context
//! assembler's plan when the request is assembled.
use super::dto::SourceDto;
use super::*;

mod assemble;
mod budget;
mod carry;
mod classify;
mod finalize;
mod generate;
mod prepare;
mod retrieve;

#[cfg(test)]
mod tests;

/// Run one chat turn.
///
/// Maintains persistent conversation history with token-aware context window
/// management, and creates the conversation when none is named.
///
/// # Errors
///
/// * `AppError::NoActiveModel` - No chat model is active
/// * `AppError::RateLimitExceeded` - Too many requests (10/min)
/// * `AppError::InvalidInput` - Empty or invalid message, or a message the
///   model's window cannot hold
/// * `AppError::Database` - Database operation failed
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: 10 requests per minute
/// - **Input Validation**: Message length and content validation
/// - **Audit Logging (CWE-778)**: Logs chat interactions with context size
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_turn(
    container: &dyn ChatRuntime,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    request_id: Option<String>,
    attachment_names: Option<Vec<String>>,
    attachment_document_ids: Option<Vec<String>>,
    emit: ChatEventSink,
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

    tracing::info!(
        conversation_id = conversation_id.as_deref().unwrap_or("NEW"),
        content_length = message.len(),
        "chat_with_conversation: START"
    );

    let flow_start = Instant::now();
    let mut metrics = ConversationFlowTimingMetrics::default();
    let preferences = tool_preferences.as_ref();

    let turn = prepare::prepare_turn(
        container,
        conversation_id,
        &message,
        request_id,
        preferences,
        &emit,
        &mut metrics,
    )
    .await?;
    let flags = classify::classify_turn(container, &turn, preferences).await;
    let mut evidence = turn.budget.evidence;
    let carried = carry::carry_material(
        container,
        &turn,
        flags,
        attachment_names.unwrap_or_default(),
        attachment_document_ids.unwrap_or_default(),
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

    let prompt = assemble::render_prompt(
        &turn,
        flags,
        preferences,
        carried,
        retrieved,
        &emit,
        &mut metrics,
    );
    let persist_start = Instant::now();
    let (user_message_id, message_tokens) = persist_user_message_pending(
        &turn.conv_service,
        &turn.conv_id,
        &turn.turn_id,
        &turn.message,
        &prompt.attachment_names,
        &prompt.attachment_ids,
        &turn.llm,
    )
    .await?;
    metrics.persist_user_message_ms = elapsed_ms(persist_start);

    // From here a failure leaves the saved question retryable rather than
    // pending forever, so every error goes through the same cleanup.
    let generated =
        match assemble::plan_request(container, &turn, flags, preferences, &prompt, &mut metrics)
            .await
        {
            Ok(request) => {
                generate::generate_answer(
                    container,
                    &turn,
                    flags,
                    prompt,
                    request,
                    &emit,
                    &mut metrics,
                )
                .await
            }
            Err(error) => Err(error),
        };
    match generated {
        Ok(generated) => {
            finalize::finalize_turn(
                container,
                &turn,
                flags,
                generated,
                route.record,
                &user_message_id,
                message_tokens,
                &emit,
                flow_start,
                metrics,
            )
            .await
        }
        Err(error) => {
            tracing::error!(
                conversation_id = turn.conv_id.as_str(),
                error = %error,
                "chat_with_conversation: LLM generation FAILED"
            );
            mark_user_message_failed(&turn.conv_service, &user_message_id).await;
            metrics.total_ms = elapsed_ms(flow_start);
            finalize::log_flow_timing(&turn.conv_id, &metrics, true);
            Err(error)
        }
    }
}
