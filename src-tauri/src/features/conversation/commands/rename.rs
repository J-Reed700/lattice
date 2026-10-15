//! Renaming a conversation.

use crate::interfaces::di::container::Container;
use lattice_core::shared::error::{AppError, Result};
use tauri::State;

/// Renames a conversation with a new title
///
/// Updates the conversation's title to better reflect its content or purpose.
/// Useful when the conversation's topic evolves or when the initial title was
/// generic. The title is validated and sanitized to prevent XSS attacks.
///
/// # Arguments
///
/// * `container` - Service container with conversation service and security context
/// * `conversation_id` - Unique identifier of the conversation to rename
/// * `new_title` - New title for the conversation (will be sanitized)
///
/// # Returns
///
/// * `Ok(())` - Conversation successfully renamed
/// * `Err(AppError)` - If rate limited, validation fails, or conversation not found
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many rename requests (rate limited)
/// * `AppError::InvalidInput` - Title validation failed (empty, too long, or XSS detected)
/// * `AppError::NotFound` - Conversation ID not found
/// * `AppError::Other` - Database update failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Rename a conversation
/// await invoke('rename_conversation', {
///   conversationId: 'conv_abc123',
///   newTitle: 'Machine Learning Discussion - Advanced Topics'
/// });
///
/// console.log('Conversation renamed successfully');
///
/// // Verify rename
/// const conversation = await invoke<Conversation>('get_conversation', {
///   conversationId: 'conv_abc123'
/// });
///
/// console.log(`New title: ${conversation.title}`);
/// ```
///
/// # Title Validation
///
/// Titles are validated and sanitized:
/// - **Min Length**: 1 character (empty titles rejected)
/// - **Max Length**: 500 characters (truncated if longer)
/// - **XSS Prevention**: HTML/script tags removed
/// - **Whitespace**: Leading/trailing whitespace trimmed
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents rename spam
/// - **Input Validation**: Title sanitized to prevent XSS attacks
///
/// # Use Cases
///
/// - **Topic Evolution**: Update title as conversation topic changes
/// - **Better Organization**: Use descriptive titles for easy identification
/// - **Searchability**: Clear titles make conversations easier to find
/// - **Context Switching**: Rename to reflect current discussion focus
///
/// # Best Practices
///
/// **Good Titles**:
/// - "Python Async Best Practices"
/// - "Q3 Financial Analysis - Revenue Trends"
/// - "Research: Quantum Computing Applications"
///
/// **Poor Titles**:
/// - "Conversation 1" (too generic)
/// - "Question" (not descriptive)
/// - "..." (empty or whitespace only)
///
/// # Why Rename?
///
/// Conversations often start with generic titles like "New Conversation" or
/// "Quick Question". As the conversation evolves, renaming helps:
/// 1. **Identify conversations** at a glance in the list view
/// 2. **Search effectively** by remembering topic keywords
/// 3. **Organize knowledge** by grouping related conversations
/// 4. **Context retrieval** when resuming a conversation later
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Validates and sanitizes title input (XSS prevention)
/// 3. Delegates to `ConversationService.rename_conversation()`
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation (sanitize title, prevent XSS)
/// 3. Execute ConversationService.rename_conversation()
/// 4. Return success
///
/// Implementation function (Pure Rust - No Tauri)
pub async fn rename_conversation_impl(
    container: &Container,
    conversation_id: String,
    new_title: String,
) -> Result<()> {
    // 1. Rate limiting
    container
        .security_context()
        .rate_limiters()
        .llm_question
        .check_rate_limit("rename_conversation")
        .await
        .map_err(|e| AppError::Other(format!("Rate limit exceeded: {}", e)))?;

    // 2. Input validation
    let sanitized_title = container
        .security_context()
        .input_validator()
        .validate_search_query(&new_title)
        .map_err(|e| AppError::Other(format!("Invalid title: {}", e)))?;

    // 3. Delegate to service
    let service = container.conversation_service();
    service
        .rename_conversation(&conversation_id, sanitized_title.clone())
        .await?;

    Ok(())
}

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id, new_title = %new_title))]
pub async fn rename_conversation(
    container: State<'_, Container>,
    conversation_id: String,
    new_title: String,
) -> Result<()> {
    rename_conversation_impl(container.inner(), conversation_id, new_title).await
}
