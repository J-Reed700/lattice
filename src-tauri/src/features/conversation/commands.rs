//! Conversation Management Commands (DDD Architecture)
//!
//! Thin controllers for managing LLM conversations and conversational question-answering.
//! Conversations maintain context across multiple Q&A interactions, enabling follow-up
//! questions and contextual understanding. These commands follow Domain-Driven Design
//! principles with cross-cutting concerns (rate limiting, validation)
//! and business logic delegation to ConversationService.
//!
//! # Architecture
//!
//! Commands are **thin controllers** that delegate to the service layer:
//! - **Rate limiting**: Protect against resource exhaustion
//! - **Input validation**: Sanitize and validate user inputs (XSS prevention)
//! - **Service delegation**: Business logic in ConversationService
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
//! 5. Return result

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::interfaces::di::container::Container;
use lattice_core::domain::Conversation;
use lattice_core::shared::error::{AppError, Result};
use tauri::State;

mod delete;
mod messages;
mod rename;
mod space;

pub use delete::{delete_conversation, delete_conversation_impl};
pub use messages::{get_conversation_messages, get_conversation_messages_impl};
pub use rename::{rename_conversation, rename_conversation_impl};
pub use space::create_conversation_space_impl;

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
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation (title sanitization)
/// 3. Execute ConversationService.create_conversation()
/// 4. Return conversation metadata
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
    runtime: &dyn ChatRuntime,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<Conversation>> {
    // 1. Rate limiting
    runtime.check_read_rate_limit("list_conversations").await?;

    // 2. Validate and cap limit
    let limit = limit.map(|l| l.min(100)); // Cap at 100

    // 3. Delegate to service
    let service = runtime.conversation_service();
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
    runtime: &dyn ChatRuntime,
    conversation_id: String,
) -> Result<Option<Conversation>> {
    // 1. Rate limiting
    runtime.check_read_rate_limit("get_conversation").await?;

    // 2. Delegate to service
    let service = runtime.conversation_service();
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
