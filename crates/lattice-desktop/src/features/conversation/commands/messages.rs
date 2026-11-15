//! Reading a conversation's messages.

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::conversation::chat::reply_text::ReplyText;
use crate::features::conversation::chat::ReplyPieceDto;
use crate::interfaces::di::container::Container;
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::domain::conversation::MessageRole;
use lattice_core::domain::ConversationMessage;
use lattice_core::shared::error::{AppError, Result};
use std::collections::HashMap;
use tauri::State;

/// All messages of a conversation, oldest first.
///
/// # Errors
///
/// * `AppError::NotFound` - Conversation ID not found
/// * `AppError::Other` - Database query failed
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

    // 3. Return messages from aggregate
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

/// What each assistant message is as the frontend draws it, by message id: its
/// saved references, or for a message saved before them its old `[n]` read once
/// against that message's own sources, which are named in the conversation's
/// scope in the order the messages come. Nothing is written back to a message.
pub async fn replies_of(
    runtime: &dyn ChatRuntime,
    conversation_id: &str,
    messages: &[ConversationMessage],
) -> Result<HashMap<String, Vec<ReplyPieceDto>>> {
    let names = runtime.kept_names().open(conversation_id).await?;
    let mut replies = HashMap::new();
    for message in messages
        .iter()
        .filter(|message| matches!(message.role, MessageRole::Assistant))
    {
        let metadata: serde_json::Value = message
            .metadata
            .as_deref()
            .and_then(|metadata| serde_json::from_str(metadata).ok())
            .unwrap_or(serde_json::Value::Null);
        let reply = ReplyText::stored(&message.content, &metadata).unwrap_or_else(|| {
            let sources: Vec<SourceDto> = metadata
                .get("sources")
                .and_then(|sources| serde_json::from_value(sources.clone()).ok())
                .unwrap_or_default();
            ReplyText::legacy(&names, &message.content, &sources)
        });
        replies.insert(message.id.clone(), reply.pieces());
    }
    names.save().await?;
    Ok(replies)
}

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id))]
pub async fn get_conversation_messages(
    container: State<'_, Container>,
    conversation_id: String,
) -> Result<Vec<ConversationMessage>> {
    get_conversation_messages_impl(container.inner(), conversation_id).await
}
