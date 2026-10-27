//! Deleting a conversation.

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::interfaces::di::container::Container;
use lattice_core::shared::error::Result;
use tauri::State;

/// Permanently deletes a conversation and all its messages.
///
/// # Errors
///
/// * `AppError::NotFound` - Conversation ID not found, including on a second delete
/// * `AppError::Other` - Database deletion failed
pub async fn delete_conversation_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
) -> Result<()> {
    runtime.delete_conversation(&conversation_id).await
}

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id))]
pub async fn delete_conversation(
    container: State<'_, Container>,
    conversation_id: String,
) -> Result<()> {
    delete_conversation_impl(container.inner(), conversation_id).await
}
