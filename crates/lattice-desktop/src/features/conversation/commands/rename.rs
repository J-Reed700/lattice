//! Renaming a conversation.

use crate::interfaces::di::container::Container;
use lattice_core::shared::error::{AppError, Result};
use tauri::State;

/// Validates and sanitizes the new title before updating the conversation.
pub async fn rename_conversation_impl(
    container: &Container,
    conversation_id: String,
    new_title: String,
) -> Result<()> {
    let sanitized_title = container
        .security_context()
        .input_validator()
        .validate_search_query(&new_title)
        .map_err(|e| AppError::Other(format!("Invalid title: {}", e)))?;

    let service = container.conversation_service();
    service
        .rename_conversation(&conversation_id, sanitized_title.clone())
        .await?;

    Ok(())
}

#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id, new_title = %new_title))]
pub async fn rename_conversation(
    container: State<'_, Container>,
    conversation_id: String,
    new_title: String,
) -> Result<()> {
    rename_conversation_impl(container.inner(), conversation_id, new_title).await
}
