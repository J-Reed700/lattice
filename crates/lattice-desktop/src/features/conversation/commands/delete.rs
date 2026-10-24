//! Deleting a conversation.

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::interfaces::di::container::Container;
use lattice_core::shared::error::Result;
use tauri::State;

/// Permanently removes a conversation and its messages.
/// Returns `NotFound` if the conversation ID does not exist.
pub async fn delete_conversation_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
) -> Result<()> {
    runtime.delete_conversation(&conversation_id).await
}

#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id))]
pub async fn delete_conversation(
    container: State<'_, Container>,
    conversation_id: String,
) -> Result<()> {
    delete_conversation_impl(container.inner(), conversation_id).await
}
