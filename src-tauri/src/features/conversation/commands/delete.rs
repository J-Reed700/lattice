//! Deleting a conversation.

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::interfaces::di::container::Container;
use crate::shared::error::Result;
use tauri::State;

/// Deletes a conversation and all associated messages
///
/// Permanently removes a conversation from the system including all message history,
/// metadata, and token usage records. This operation cannot be undone. Use when a
/// conversation is no longer needed or to clean up test/experimental conversations.
///
/// # Arguments
///
/// * `container` - Service container with conversation service and security context
/// * `conversation_id` - Unique identifier of the conversation to delete
///
/// # Returns
///
/// * `Ok(())` - Conversation successfully deleted
/// * `Err(AppError)` - If rate limited, conversation not found, or deletion fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many deletion requests (rate limited)
/// * `AppError::NotFound` - Conversation ID not found
/// * `AppError::Other` - Database deletion failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Delete a conversation
/// await invoke('delete_conversation', {
///   conversationId: 'conv_abc123'
/// });
///
/// console.log('Conversation deleted successfully');
///
/// // Verify deletion
/// const conversation = await invoke<Conversation | null>('get_conversation', {
///   conversationId: 'conv_abc123'
/// });
///
/// console.log(conversation === null); // true
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents mass deletion attacks
///
/// # Cascade Deletion
///
/// When a conversation is deleted, the following data is also removed:
/// - **All Messages**: User questions and assistant responses
/// - **Message Metadata**: Timestamps, token counts, role information
/// - **Conversation Metadata**: Title, model name, system prompt
/// - **Aggregate State**: Total message count, total tokens
///
/// **Not Deleted**: Search index documents (conversation messages are not indexed)
///
/// # Use Cases
///
/// - **Privacy**: Remove sensitive conversations
/// - **Cleanup**: Delete test or experimental conversations
/// - **Organization**: Remove outdated or irrelevant conversations
/// - **Storage Management**: Free up database space
///
/// # Warning
///
/// **This operation is irreversible.** Once deleted, conversation history cannot be
/// recovered. Consider the following before deleting:
/// 1. **Export First**: Save important conversations to file
/// 2. **Confirm Intent**: Add UI confirmation dialog
/// 3. **Batch Carefully**: Be cautious when deleting multiple conversations
///
/// # Best Practices
///
/// **When to Delete**:
/// - Test conversations from development
/// - Conversations with incorrect or irrelevant responses
/// - Duplicate conversations
/// - Conversations with sensitive information (after using data)
///
/// **When to Keep**:
/// - Conversations with valuable insights
/// - Reference conversations for similar future questions
/// - Conversations demonstrating good LLM patterns
///
/// # Idempotency
///
/// Deleting an already-deleted conversation returns `NotFound` error. This is
/// intentional to help detect bugs where the same conversation is deleted twice.
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Delegates to `ConversationService.delete_conversation()`
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute ConversationService.delete_conversation() (cascade delete)
/// 3. Return success
///
/// # Side Effects
///
/// - Removes conversation from database
/// - Removes all associated messages
/// - Does NOT affect search index (conversations are not indexed)
///
/// Implementation function (Pure Rust - No Tauri)
pub async fn delete_conversation_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
) -> Result<()> {
    // 1. Rate limiting
    runtime
        .check_delete_rate_limit("delete_conversation")
        .await?;
    // 2. Stop its turn, take its attachments and remove it
    runtime
        .delete_conversation_unmetered(&conversation_id)
        .await
}

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id))]
pub async fn delete_conversation(
    container: State<'_, Container>,
    conversation_id: String,
) -> Result<()> {
    delete_conversation_impl(container.inner(), conversation_id).await
}
