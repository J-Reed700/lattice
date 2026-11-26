//! A conversation's messages as they are read back.

use super::ports::ChatRuntime;
use super::reply_support;
use super::reply_text::ReplyText;
use crate::features::conversation::dto::MessageDto;
use crate::features::conversation::ConversationServiceTrait;
use lattice_core::application::contracts::replies::ReplyPieceDto;
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::domain::conversation::MessageRole;
use lattice_core::domain::ConversationMessage;
use lattice_core::shared::error::{AppError, Result};
use std::collections::HashMap;
use std::sync::Arc;

/// All messages of a conversation, oldest first.
///
/// # Errors
///
/// * `AppError::NotFound` - Conversation ID not found
/// * `AppError::Other` - Database query failed
pub async fn saved_messages(
    runtime: &dyn ChatRuntime,
    conversation_id: &str,
) -> Result<Vec<ConversationMessage>> {
    read(runtime, &runtime.conversation_service(), conversation_id).await
}

async fn read(
    runtime: &dyn ChatRuntime,
    conversations: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
) -> Result<Vec<ConversationMessage>> {
    let aggregate = conversations
        .get_conversation(conversation_id)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

    let mut messages = aggregate.messages().to_vec();
    for message in &mut messages {
        message.metadata = super::source_snapshots::hydrate_message_metadata(
            runtime,
            conversation_id,
            message.metadata.take(),
        )
        .await;
        message.metadata = claims_without_old_markers(message.metadata.take());
    }
    Ok(messages)
}

/// A message saved before references stored each checked sentence with its old
/// `[n]` markers; they are taken out of those sentences here, for the frontend.
fn claims_without_old_markers(metadata: Option<String>) -> Option<String> {
    let raw = metadata.as_deref()?;
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return metadata;
    };
    if value.get("references").is_some() {
        return metadata;
    }
    let Some(verdicts) = value
        .get_mut("verification")
        .and_then(|verification| verification.get_mut("claimVerdicts"))
        .and_then(serde_json::Value::as_array_mut)
    else {
        return metadata;
    };
    let mut changed = false;
    for verdict in verdicts {
        let Some(serde_json::Value::String(sentence)) = verdict.get_mut("sentence") else {
            continue;
        };
        let clean = lattice_core::shared::citation_markers::without_citation_markers(sentence);
        if clean != *sentence {
            *sentence = clean;
            changed = true;
        }
    }
    changed.then(|| value.to_string()).or(metadata)
}

/// What each assistant message is as the frontend draws it, by message id: its
/// saved references, or for a message saved before them its old `[n]` read once
/// against that message's own sources, which are named in the conversation's
/// scope in the order the messages come. Nothing is written back to a message.
async fn replies_of(
    runtime: &dyn ChatRuntime,
    conversation_id: &str,
    messages: &[ConversationMessage],
) -> Result<HashMap<String, Vec<ReplyPieceDto>>> {
    let names = runtime.kept_names().open(conversation_id).await?;
    let source = runtime
        .turn_source(conversation_id, None, Arc::clone(&names))
        .await;
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
        let sources: Vec<SourceDto> = metadata
            .get("sources")
            .and_then(|sources| serde_json::from_value(sources.clone()).ok())
            .unwrap_or_default();
        let reply = ReplyText::stored(&message.content, &metadata).unwrap_or_else(|| {
            ReplyText::legacy(&names, source.as_ref(), &message.content, &sources)
        });
        replies.insert(
            message.id.clone(),
            reply_support::pieces(&reply, &sources, metadata.get("verification")),
        );
    }
    names.save().await?;
    Ok(replies)
}

/// The conversation's messages as the frontend draws them, each assistant
/// message with its pieces. Whatever hands the frontend a conversation's
/// messages reads them here.
pub async fn shown_messages(
    runtime: &dyn ChatRuntime,
    conversations: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
) -> Result<Vec<MessageDto>> {
    let messages = read(runtime, conversations, conversation_id).await?;
    let mut replies = replies_of(runtime, conversation_id, &messages).await?;
    Ok(messages
        .iter()
        .map(|message| MessageDto::of(message, replies.remove(&message.id)))
        .collect())
}
