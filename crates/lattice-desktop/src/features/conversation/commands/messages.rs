//! Reading a conversation's messages.

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::interfaces::di::container::Container;
use lattice_core::domain::ConversationMessage;
use lattice_core::shared::error::{AppError, Result};
use tauri::State;

/// Retrieves all messages for a conversation in chronological order with hydrated metadata.
pub async fn get_conversation_messages_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
) -> Result<Vec<ConversationMessage>> {
    let service = runtime.conversation_service();
    let aggregate = service
        .get_conversation(&conversation_id)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

    let mut messages = aggregate.messages().to_vec();
    for message in &mut messages {
        message.metadata =
            crate::features::conversation::chat::source_snapshots::hydrate_message_metadata(
                runtime,
                &conversation_id,
                message.metadata.take(),
            )
            .await;
    }
    Ok(messages)
}

#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id))]
pub async fn get_conversation_messages(
    container: State<'_, Container>,
    conversation_id: String,
) -> Result<Vec<ConversationMessage>> {
    get_conversation_messages_impl(container.inner(), conversation_id).await
}
