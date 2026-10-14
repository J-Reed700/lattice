//! Reading a conversation's messages.

use crate::domain::ConversationMessage;
use crate::features::conversation::chat::ports::ChatRuntime;
use crate::interfaces::di::container::Container;
use crate::shared::error::{AppError, Result};
use tauri::State;

/// Retrieves all messages for a conversation
///
/// Fetches the complete message history for a conversation in chronological order.
/// Messages include both user questions and assistant responses with timestamps,
/// token counts, and metadata. Use this to display conversation history in the UI
/// or to understand the full context of a conversation.
///
/// # Arguments
///
/// * `container` - Service container with conversation service and security context
/// * `conversation_id` - Unique identifier of the conversation
///
/// # Returns
///
/// * `Ok(Vec<ConversationMessage>)` - List of messages in chronological order
/// * `Err(AppError)` - If rate limited, conversation not found, or query fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many message fetch requests (rate limited)
/// * `AppError::NotFound` - Conversation ID not found
/// * `AppError::Other` - Database query failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface ConversationMessage {
///   id: string;
///   conversationId: string;
///   role: 'user' | 'assistant' | 'system';
///   content: string;
///   tokens: number;
///   createdAt: string;
///   metadata?: Record<string, any>;
/// }
///
/// // Get all messages for a conversation
/// const messages = await invoke<ConversationMessage[]>('get_conversation_messages', {
///   conversationId: 'conv_abc123'
/// });
///
/// console.log(`Conversation has ${messages.length} messages`);
///
/// // Display conversation history
/// messages.forEach(msg => {
///   if (msg.role === 'user') {
///     console.log(`User: ${msg.content}`);
///   } else if (msg.role === 'assistant') {
///     console.log(`Assistant: ${msg.content} (${msg.tokens} tokens)`);
///   }
/// });
///
/// // Calculate total tokens
/// const totalTokens = messages.reduce((sum, msg) => sum + msg.tokens, 0);
/// console.log(`Total tokens: ${totalTokens}`);
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents excessive message fetching
///
/// # Message Ordering
///
/// Messages are returned in chronological order (oldest first):
/// 1. Initial user question
/// 2. Assistant response
/// 3. Follow-up user question
/// 4. Assistant response
///
/// The sequence continues in the same alternating order.
///
/// This ordering makes it easy to reconstruct the conversation flow for display.
///
/// # Performance Considerations
///
/// For very long conversations (hundreds of messages):
/// - Consider implementing message pagination in the UI
/// - Load recent messages first, older messages on demand
/// - Cache messages to avoid repeated fetches
///
/// # Use Cases
///
/// - **Conversation Display**: Show full message history in chat UI
/// - **Context Window**: Provide context for follow-up questions
/// - **Export**: Save conversation history to file
/// - **Analysis**: Analyze conversation patterns and token usage
/// - **Debugging**: Inspect LLM responses and context flow
///
/// # Message Roles
///
/// - **user**: Human-written questions/prompts
/// - **assistant**: LLM-generated responses
/// - **system**: System-level instructions (e.g., system prompt)
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Delegates to `ConversationService.get_conversation()`
/// 3. Extracts messages from conversation aggregate
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute ConversationService.get_conversation()
/// 3. If conversation not found, return NotFound error
/// 4. Extract messages from aggregate
/// 5. Return messages in chronological order
///
/// Implementation function (Pure Rust - No Tauri)
pub async fn get_conversation_messages_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
) -> Result<Vec<ConversationMessage>> {
    // 1. Rate limiting
    runtime
        .check_read_rate_limit("get_conversation_messages")
        .await?;

    // 2. Delegate to service
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

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id))]
pub async fn get_conversation_messages(
    container: State<'_, Container>,
    conversation_id: String,
) -> Result<Vec<ConversationMessage>> {
    get_conversation_messages_impl(container.inner(), conversation_id).await
}
