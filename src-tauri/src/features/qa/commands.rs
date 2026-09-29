//! Q&A command handlers: the model and health checks the chat surface uses.
//!
//! Question answering itself lives in the conversation chat; the standalone
//! `ask_question` commands were removed with the use case behind them.

use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use tauri::State;

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
