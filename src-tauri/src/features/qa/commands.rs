//! Q&A command handlers: the LLM health check the chat surface uses.
//!
//! Question answering itself lives in the conversation chat; the standalone
//! `ask_question` commands were removed with the use case behind them.

use crate::interfaces::di::Container;
use lattice_core::shared::error::Result;
use tauri::State;

/// Checks LLM availability and returns health status information
///
/// Verifies that the Settings default chat model is ready and returns
/// detailed information about the model's capabilities and current status. Useful
/// for displaying service health in UI, debugging connectivity issues, and gracefully
/// handling LLM unavailability.
pub async fn check_llm_health(
    container: State<'_, Container>,
) -> Result<super::dto::LLMHealthStatusDto> {
    let pinned = container.pinned_default_chat_llm().await?;
    let llm = &pinned.llm;
    let is_ready = llm.is_ready().await?;

    let health_response = super::dto::LLMHealthStatusDto {
        available: is_ready,
        model: pinned.resolved.model.clone(),
        model_name: llm.model_name().to_string(),
        max_context_tokens: llm.max_context_tokens(),
        backend: "rust-ddd".to_string(),
    };

    Ok(health_response)
}
