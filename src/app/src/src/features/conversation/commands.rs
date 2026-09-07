//! Conversation Management Commands (DDD Architecture)
//!
//! Thin controllers for managing LLM conversations and conversational question-answering.
//! Conversations maintain context across multiple Q&A interactions, enabling follow-up
//! questions and contextual understanding. These commands follow Domain-Driven Design
//! principles with cross-cutting concerns (rate limiting, validation, audit logging)
//! and business logic delegation to ConversationService.
//!
//! # Architecture
//!
//! Commands are **thin controllers** that delegate to the service layer:
//! - **Rate limiting**: Protect against resource exhaustion
//! - **Input validation**: Sanitize and validate user inputs (XSS prevention)
//! - **Service delegation**: Business logic in ConversationService
//! - **Audit logging**: Track security-sensitive operations
//!
//! # Conversational Q&A Pattern
//!
//! Conversations enable stateful Q&A:
//! 1. **Create conversation** with title and model selection
//! 2. **Ask questions** that reference previous context
//! 3. **Maintain message history** for context window
//! 4. **Manage conversations** (list, rename, delete)
//!
//! # Command Pattern
//!
//! Each command follows this structure (< 50 lines):
//! 1. Rate limiting check
//! 2. Input validation
//! 3. Get service from container
//! 4. Delegate to service
//! 5. Audit logging (for important operations)
//! 6. Return result

use crate::domain::{Conversation, ConversationMessage};
use crate::features::conversation::space_dto::{
    ConversationSpaceDto, CreateConversationSpaceRequestDto,
};
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::container::Container;
use crate::shared::error::{AppError, Result};
use chrono::Utc;
use serde_json::Value;
use tauri::State;
use tracing::warn;

// ============================================================================
// Conversation CRUD Commands
// ============================================================================

/// Creates a new conversation for multi-turn Q&A with context
///
/// Initializes a new conversation thread that maintains context across multiple
/// question-answering interactions. Conversations remember previous messages,
/// enabling follow-up questions and contextual understanding. This is essential
/// for natural, flowing Q&A where each question builds on previous answers.
///
/// # Arguments
///
/// * `container` - Service container with conversation service and security context
/// * `title` - Human-readable conversation title
/// * `model_name` - LLM model to use (e.g., "gpt-4", "claude-3-opus")
/// * `system_prompt` - Optional system instructions to guide LLM behavior
///
/// # Returns
///
/// * `Ok(Conversation)` - Created conversation with ID and metadata
/// * `Err(AppError)` - If rate limited, validation fails, or creation fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many conversation creation requests
/// * `AppError::InvalidInput` - Title validation failed (XSS prevention)
/// * `AppError::Other` - Database error or service unavailable
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface Conversation {
///   id: string;
///   title: string;
///   modelName: string;
///   systemPrompt?: string;
///   createdAt: string;
///   updatedAt: string;
///   messageCount: number;
///   totalTokens: number;
/// }
///
/// // Create a new conversation
/// const conversation = await invoke<Conversation>('create_conversation', {
///   title: 'Research Discussion',
///   modelName: 'gpt-4',
///   systemPrompt: 'You are a helpful research assistant focused on academic papers.'
/// });
///
/// console.log(`Created conversation: ${conversation.id}`);
/// console.log(`Title: ${conversation.title}`);
///
/// // Now use this conversation for Q&A
/// const answer = await invoke('ask_with_conversation', {
///   conversationId: conversation.id,
///   question: 'What are the key findings in quantum computing research?'
/// });
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents conversation spam
/// - **Input Validation**: Title sanitized to prevent XSS attacks
/// - **Audit Logging (CWE-778)**: Logs conversation creation for compliance
///
/// # System Prompts
///
/// System prompts guide LLM behavior and tone:
/// - **Research Assistant**: "You are a helpful research assistant..."
/// - **Code Helper**: "You are an expert programmer who explains code clearly..."
/// - **Summarizer**: "You provide concise summaries of complex topics..."
/// - **Custom**: Any instructions to customize LLM personality
///
/// # Conversation Lifecycle
///
/// 1. **Create**: Initialize with title and model
/// 2. **Ask Questions**: Add messages with context
/// 3. **Maintain History**: Messages preserved for context window
/// 4. **Rename**: Update title as conversation evolves
/// 5. **Delete**: Remove conversation and all messages
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Validates and sanitizes title input
/// 3. Delegates to `ConversationService.create_conversation()`
/// 4. Logs audit event
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation (title sanitization)
/// 3. Execute ConversationService.create_conversation()
/// 4. Log audit event (conversation created)
/// 5. Return conversation metadata
///
/// Implementation function (Pure Rust - No Tauri)
pub async fn create_conversation_impl(
    container: &Container,
    title: String,
    model_name: String,
    system_prompt: Option<String>,
) -> Result<Conversation> {
    // 1. Rate limiting
    container
        .security_context()
        .rate_limiters()
        .llm_question
        .check_rate_limit("create_conversation")
        .await
        .map_err(|e| AppError::Other(format!("Rate limit exceeded: {}", e)))?;

    // 2. Input validation
    let sanitized_title = container
        .security_context()
        .input_validator()
        .validate_search_query(&title)
        .map_err(|e| AppError::Other(format!("Invalid title: {}", e)))?;

    // 3. Delegate to service
    let service = container.conversation_service();
    let conversation = service
        .create_conversation(sanitized_title, model_name, system_prompt)
        .await?;

    // 4. Audit logging
    let audit_logger = get_audit_logger();
    let event = AuditEvent::new(AuditAction::QuestionAnswered, AuditResult::success())
        .with_resource_id(conversation.id.to_string())
        .with_metadata("operation", "create_conversation")
        .with_metadata("title", &conversation.title);

    if let Err(e) = audit_logger.log(event).await {
        warn!("Failed to write audit log: {}", e);
    }

    Ok(conversation)
}

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(title = %title, model = %model_name))]
pub async fn create_conversation(
    container: State<'_, Container>,
    title: String,
    model_name: String,
    system_prompt: Option<String>,
) -> Result<Conversation> {
    create_conversation_impl(container.inner(), title, model_name, system_prompt).await
}

/// Lists all conversations with pagination support
///
/// Retrieves all conversations ordered by most recently updated first. Supports
/// pagination for efficient loading of large conversation lists. Returns metadata
/// only (title, model, timestamps, counts) - use `get_conversation_messages` to
/// retrieve actual message history.
///
/// # Arguments
///
/// * `container` - Service container with conversation service and security context
/// * `limit` - Maximum number of conversations to return (default: 100, capped at 100)
/// * `offset` - Number of conversations to skip for pagination (default: 0)
///
/// # Returns
///
/// * `Ok(Vec<Conversation>)` - List of conversation metadata (no messages)
/// * `Err(AppError)` - If rate limited or database query fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many list requests (rate limited)
/// * `AppError::Other` - Database query failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface Conversation {
///   id: string;
///   title: string;
///   modelName: string;
///   systemPrompt?: string;
///   createdAt: string;
///   updatedAt: string;
///   messageCount: number;
///   totalTokens: number;
/// }
///
/// // List first 20 conversations
/// const conversations = await invoke<Conversation[]>('list_conversations', {
///   limit: 20,
///   offset: 0
/// });
///
/// console.log(`Found ${conversations.length} conversations`);
///
/// // Paginate through conversations
/// let offset = 0;
/// const pageSize = 20;
/// while (true) {
///   const page = await invoke<Conversation[]>('list_conversations', {
///     limit: pageSize,
///     offset: offset
///   });
///
///   if (page.length === 0) break;
///
///   // Process page
///   page.forEach(conv => {
///     console.log(`${conv.title}: ${conv.messageCount} messages, ${conv.totalTokens} tokens`);
///   });
///
///   offset += pageSize;
/// }
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents excessive list requests
/// - **Limit Capping**: Automatically caps limit at 100 to prevent DoS
///
/// # Pagination Pattern
///
/// For efficient loading of large conversation lists:
/// 1. **Initial Load**: Fetch first page with `limit=20, offset=0`
/// 2. **Scroll/Load More**: Fetch next page with `offset += limit`
/// 3. **End Detection**: Empty array indicates no more conversations
///
/// # Ordering
///
/// Conversations are ordered by `updated_at DESC` (most recent first).
/// This ensures active conversations appear at the top of the list.
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Caps limit at 100 (security)
/// 3. Delegates to `ConversationService.list_conversations()`
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Validate and cap limit (≤ 100)
/// 3. Execute ConversationService.list_conversations()
/// 4. Return conversation metadata list
///
/// Implementation function (Pure Rust - No Tauri)
pub async fn list_conversations_impl(
    container: &Container,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<Conversation>> {
    // 1. Rate limiting
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("list_conversations")
        .await
        .map_err(|e| AppError::Other(format!("Rate limit exceeded: {}", e)))?;

    // 2. Validate and cap limit
    let limit = limit.map(|l| l.min(100)); // Cap at 100

    // 3. Delegate to service
    let service = container.conversation_service();
    service.list_conversations(limit, offset).await
}

/// Tauri command wrapper
#[tracing::instrument(skip(container))]
pub async fn list_conversations(
    container: State<'_, Container>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<Conversation>> {
    list_conversations_impl(container.inner(), limit, offset).await
}

/// Retrieves a single conversation by ID
///
/// Fetches conversation metadata (title, model, timestamps, message count, token usage)
/// without loading the actual message history. Use `get_conversation_messages` to
/// retrieve the full message history. Returns `None` if conversation ID not found.
///
/// # Arguments
///
/// * `container` - Service container with conversation service and security context
/// * `conversation_id` - Unique identifier of the conversation to retrieve
///
/// # Returns
///
/// * `Ok(Some(Conversation))` - Conversation found, returns metadata
/// * `Ok(None)` - Conversation ID not found
/// * `Err(AppError)` - If rate limited or database query fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many get requests (rate limited)
/// * `AppError::Other` - Database query failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface Conversation {
///   id: string;
///   title: string;
///   modelName: string;
///   systemPrompt?: string;
///   createdAt: string;
///   updatedAt: string;
///   messageCount: number;
///   totalTokens: number;
/// }
///
/// // Get conversation metadata
/// const conversation = await invoke<Conversation | null>('get_conversation', {
///   conversationId: 'conv_abc123'
/// });
///
/// if (conversation) {
///   console.log(`Conversation: ${conversation.title}`);
///   console.log(`Model: ${conversation.modelName}`);
///   console.log(`Messages: ${conversation.messageCount}`);
///   console.log(`Tokens used: ${conversation.totalTokens}`);
///
///   // Load message history separately
///   const messages = await invoke('get_conversation_messages', {
///     conversationId: conversation.id
///   });
/// } else {
///   console.log('Conversation not found');
/// }
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents excessive metadata queries
///
/// # Use Cases
///
/// - **Conversation Header**: Display title, model, stats in UI
/// - **Validation**: Check if conversation exists before operations
/// - **Stats Display**: Show message count and token usage
/// - **Lazy Loading**: Load metadata first, messages on demand
///
/// # Why Separate Messages?
///
/// Separating metadata from messages enables:
/// 1. **Performance**: Fast conversation list loading
/// 2. **Lazy Loading**: Load message history only when needed
/// 3. **Bandwidth**: Reduce data transfer for list views
/// 4. **Scalability**: Large conversations don't slow down lists
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Applies rate limiting
/// 2. Delegates to `ConversationService.get_conversation()`
/// 3. Maps aggregate to conversation DTO (metadata only)
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute ConversationService.get_conversation()
/// 3. Map aggregate to Conversation (extract metadata, discard messages)
/// 4. Return `Option<Conversation>`
///
/// Implementation function (Pure Rust - No Tauri)
pub async fn get_conversation_impl(
    container: &Container,
    conversation_id: String,
) -> Result<Option<Conversation>> {
    // 1. Rate limiting
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("get_conversation")
        .await
        .map_err(|e| AppError::Other(format!("Rate limit exceeded: {}", e)))?;

    // 2. Delegate to service
    let service = container.conversation_service();
    let aggregate = service.get_conversation(&conversation_id).await?;

    // 3. Convert aggregate to conversation (return just metadata, not messages)
    Ok(aggregate.map(|agg| Conversation {
        id: agg.id().clone(),
        title: agg.title().to_string(),
        model_name: agg.model_name().to_string(),
        system_prompt: agg.system_prompt().map(|s| s.to_string()),
        created_at: agg.created_at(),
        updated_at: agg.updated_at(),
        message_count: agg.message_count(),
        total_tokens: agg.total_tokens(),
    }))
}

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id))]
pub async fn get_conversation(
    container: State<'_, Container>,
    conversation_id: String,
) -> Result<Option<Conversation>> {
    get_conversation_impl(container.inner(), conversation_id).await
}

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
    container: &Container,
    conversation_id: String,
) -> Result<Vec<ConversationMessage>> {
    // 1. Rate limiting
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("get_conversation_messages")
        .await
        .map_err(|e| AppError::Other(format!("Rate limit exceeded: {}", e)))?;

    // 2. Delegate to service
    let service = container.conversation_service();
    let aggregate = service
        .get_conversation(&conversation_id)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

    // 3. Return messages from aggregate
    Ok(aggregate.messages().to_vec())
}

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id))]
pub async fn get_conversation_messages(
    container: State<'_, Container>,
    conversation_id: String,
) -> Result<Vec<ConversationMessage>> {
    get_conversation_messages_impl(container.inner(), conversation_id).await
}

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
/// - **Audit Logging (CWE-778)**: Logs conversation title changes for compliance
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
/// 4. Logs audit event with new title
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation (sanitize title, prevent XSS)
/// 3. Execute ConversationService.rename_conversation()
/// 4. Log audit event (conversation renamed, new title)
/// 5. Return success
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

    // 4. Audit logging
    let audit_logger = get_audit_logger();
    let event = AuditEvent::new(AuditAction::QuestionAnswered, AuditResult::success())
        .with_resource_id(&conversation_id)
        .with_metadata("operation", "rename_conversation")
        .with_metadata("new_title", &sanitized_title);

    if let Err(e) = audit_logger.log(event).await {
        warn!("Failed to write audit log: {}", e);
    }

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
/// - **Audit Logging (CWE-778)**: Logs conversation deletions for compliance
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
/// 3. Logs audit event for compliance
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Execute ConversationService.delete_conversation() (cascade delete)
/// 3. Log audit event (conversation deleted)
/// 4. Return success
///
/// # Side Effects
///
/// - Removes conversation from database
/// - Removes all associated messages
/// - Does NOT affect search index (conversations are not indexed)
///
/// Implementation function (Pure Rust - No Tauri)
pub async fn delete_conversation_impl(
    container: &Container,
    conversation_id: String,
) -> Result<()> {
    // 1. Rate limiting
    container
        .security_context()
        .rate_limiters()
        .llm_question
        .check_rate_limit("delete_conversation")
        .await
        .map_err(|e| AppError::Other(format!("Rate limit exceeded: {}", e)))?;

    // 2. Delegate to service
    let service = container.conversation_service();
    service.delete_conversation(&conversation_id).await?;

    // 3. Audit logging
    let audit_logger = get_audit_logger();
    let event = AuditEvent::new(AuditAction::QuestionAnswered, AuditResult::success())
        .with_resource_id(&conversation_id)
        .with_metadata("operation", "delete_conversation");

    if let Err(e) = audit_logger.log(event).await {
        warn!("Failed to write audit log: {}", e);
    }

    Ok(())
}

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(conversation_id = %conversation_id))]
pub async fn delete_conversation(
    container: State<'_, Container>,
    conversation_id: String,
) -> Result<()> {
    delete_conversation_impl(container.inner(), conversation_id).await
}

pub async fn create_conversation_space_impl(
    container: &Container,
    request: CreateConversationSpaceRequestDto,
) -> Result<ConversationSpaceDto> {
    if request.name.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Space name cannot be empty".to_string(),
        ));
    }
    validate_space_preferences_not_journal(
        request.tool_preferences_json.as_deref(),
        "Conversation space",
    )?;

    let trimmed_name = request.name.trim().to_string();
    let request = CreateConversationSpaceRequestDto {
        name: trimmed_name,
        ..request
    };

    crate::features::conversation::space_repository::ConversationSpaceRepository::new(
        container.db_pool().clone(),
    )
    .create(request)
    .await
}

fn validate_space_preferences_not_journal(
    raw: Option<&str>,
    context: &str,
) -> Result<(), AppError> {
    if preferences_declares_journal(raw) {
        return Err(AppError::InvalidInput(format!(
            "{} cannot declare `spaceType=journal`; journals are a separate entity",
            context
        )));
    }
    Ok(())
}

fn preferences_declares_journal(raw: Option<&str>) -> bool {
    let Some(raw_json) = raw else {
        return false;
    };
    if raw_json.trim().is_empty() {
        return false;
    }
    let parsed: Result<Value, _> = serde_json::from_str(raw_json);
    let Ok(value) = parsed else {
        return false;
    };
    let kind = value
        .get("spaceType")
        .and_then(Value::as_str)
        .or_else(|| value.get("space_type").and_then(Value::as_str))
        .unwrap_or("standard");
    kind.eq_ignore_ascii_case("journal")
}
