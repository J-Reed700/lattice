//! Q&A Command Handlers (DDD Architecture)
//!
//! **DEPRECATED**: These standalone Q&A commands have been superseded by the consolidated
//! conversation chat in `conversation_chat.rs`. The conversation chat now includes all
//! QA capabilities (HyDE, structured prompts, token budgeting, source citations, and
//! tool calling) in a streaming, multi-turn conversational interface.
//!
//! This module is retained for rollback purposes and should not be used for new development.
//!
//! ## Original Description
//!
//! Thin controllers for question-answering with Retrieval-Augmented Generation (RAG).
//! These commands apply cross-cutting concerns (rate limiting, validation, audit logging)
//! and delegate RAG pipeline logic to dedicated use cases.

use crate::features::qa::dto::{QARequestDto, QAResponseDto};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use futures::StreamExt;
use tauri::{AppHandle, Emitter, Runtime, State};

/// Answers a question using Retrieval-Augmented Generation (RAG)
///
/// Performs intelligent question-answering by retrieving relevant document chunks
/// from the search index and using them as context for LLM-based answer generation.
/// This RAG approach grounds responses in the user's indexed documents, reducing
/// hallucinations and providing source citations.
///
/// This command delegates to the non-streaming `ask_question` use case.
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Q&A request DTO with question and optional generation parameters
///
/// # Returns
///
/// * `Ok(QAResponseDto)` - Generated answer with source citations and metadata
/// * `Err(AppError)` - If rate limited, validation fails, or RAG pipeline fails
pub async fn ask_question(
    container: State<'_, Container>,
    request: QARequestDto,
) -> Result<QAResponseDto> {
    tracing::info!(question = %request.question, "Command: ask_question - ENTRY");

    // 1. Rate limiting (Q&A is expensive - LLM calls)
    container
        .security_context()
        .rate_limiters()
        .qa
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // 2. Input validation
    container
        .security_context()
        .input_validator()
        .validate_search_query(&request.question)?;

    // 3. Execute use case
    let use_case = container.ask_question_use_case().await?;
    let response = use_case.execute(request).await.map_err(|e| {
        tracing::error!(error = %e, "Command: ask_question - ERROR");
        e
    })?;

    // 4. Audit logging
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::QuestionAnswered,
        "qa",
        "sources_count" => response.sources.len().to_string().as_str()
    )
    .await
    .ok();

    tracing::info!(
        sources_count = response.sources.len(),
        "Command: ask_question - EXIT"
    );
    Ok(response)
}

/// Answers a question with streaming response (real-time LLM token generation)
///
/// This command provides real-time streaming of LLM-generated answers,
/// allowing the UI to display tokens as they are generated for better user experience.
/// It emits `llm-stream` events with chunks of the answer.
///
/// # Arguments
///
/// * `app_handle` - Tauri application handle for emitting events
/// * `container` - Service container
/// * `request` - Q&A request
///
/// # Returns
///
/// * `Ok(QAResponseDto)` - Complete response after streaming finishes
/// * `Err(AppError)` - If an error occurs
pub async fn ask_question_stream<R: Runtime>(
    app_handle: AppHandle<R>,
    container: State<'_, Container>,
    request: QARequestDto,
) -> Result<QAResponseDto> {
    tracing::info!(question = %request.question, "Command: ask_question_stream - ENTRY");

    // 1. Rate limiting
    container
        .security_context()
        .rate_limiters()
        .qa
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // 2. Input validation
    container
        .security_context()
        .input_validator()
        .validate_search_query(&request.question)?;

    // 3. Execute use case (streaming)
    let use_case = container.ask_question_use_case().await?;
    let mut stream = use_case.execute_stream(request).await.map_err(|e| {
        tracing::error!(error = %e, "Command: ask_question_stream - ERROR");
        e
    })?;

    let mut answer_acc = String::new();
    let mut sources_acc = Vec::new();

    // Consume the stream, emitting events and accumulating result
    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => {
                if let Err(e) = app_handle.emit("llm-stream", &chunk) {
                    tracing::warn!("Failed to emit qa stream event: {}", e);
                }

                // Accumulate data for final response
                match chunk {
                    crate::features::qa::dto::StreamChunkDto::Token { content } => {
                        answer_acc.push_str(&content);
                    }
                    crate::features::qa::dto::StreamChunkDto::Sources { sources } => {
                        sources_acc = sources;
                    }
                    crate::features::qa::dto::StreamChunkDto::Error { error } => {
                        tracing::error!("Stream error event: {}", error);
                        // We continue to see if we can salvage partial response or receive Done
                    }
                    _ => {} // Done event
                }
            }
            Err(e) => {
                // Emit error event before returning (using llm-stream to match frontend listener)
                let _ = app_handle.emit(
                    "llm-stream",
                    crate::features::qa::dto::StreamChunkDto::Error {
                        error: e.to_string(),
                    },
                );
                return Err(e);
            }
        }
    }

    // 4. Audit
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::QuestionAnswered,
        "qa_stream",
        "sources_count" => sources_acc.len().to_string().as_str()
    )
    .await
    .ok();

    tracing::info!(
        sources_count = sources_acc.len(),
        "Command: ask_question_stream - EXIT"
    );
    Ok(QAResponseDto {
        answer: answer_acc,
        sources: sources_acc,
        confidence: None,
        metadata: None,
    })
}

/// Retrieves the currently configured LLM model for question-answering
///
/// Returns the name/identifier of the LLM model currently being used for Q&A
/// operations. Useful for displaying model information in UI or debugging
/// which model is being used for answer generation.
pub async fn get_qa_model(container: State<'_, Container>) -> Result<String> {
    let llm = container.get_or_load_llm().await?;
    Ok(llm.model_name().to_string())
}

/// Checks LLM availability and returns health status information
///
/// Verifies that the configured LLM is ready to handle Q&A requests and returns
/// detailed information about the model's capabilities and current status. Useful
/// for displaying service health in UI, debugging connectivity issues, and gracefully
/// handling LLM unavailability.
pub async fn check_llm_health(
    container: State<'_, Container>,
) -> Result<super::dto::LLMHealthStatusDto> {
    // 1. Rate limiting
    container
        .security_context()
        .rate_limiters()
        .qa
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // 2. Check LLM service availability
    let llm = container.get_or_load_llm().await?;
    let is_ready = llm.is_ready().await?;

    // 3. Build health response
    let health_response = super::dto::LLMHealthStatusDto {
        available: is_ready,
        model: llm.model_name().to_string(),
        max_context_tokens: llm.max_context_tokens(),
        backend: "rust-ddd".to_string(),
    };

    // 4. Audit logging
    let logger = crate::audit::get_audit_logger();
    crate::audit_success!(
        logger,
        crate::audit::AuditAction::QuestionAnswered,
        "llm_health_check",
        "available" => if is_ready { "true" } else { "false" }
    )
    .await
    .ok();

    Ok(health_response)
}
