//! Thin commands for managing conversations: input validation here, business
//! logic in `ConversationService`.

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::interfaces::di::container::Container;
use lattice_core::domain::Conversation;
use lattice_core::shared::error::{AppError, Result};
use tauri::State;

mod delete;
pub mod images;
mod messages;
mod rename;
mod space;

pub use delete::{delete_conversation, delete_conversation_impl};
pub use messages::{get_conversation_messages, get_conversation_messages_impl};
pub use rename::{rename_conversation, rename_conversation_impl};
pub use space::create_conversation_space_impl;

/// Creates a conversation with a title and an optional system prompt.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Title validation failed
/// * `AppError::Other` - Database error or service unavailable
pub async fn create_conversation_impl(
    container: &Container,
    title: String,
    system_prompt: Option<String>,
) -> Result<Conversation> {
    // 1. Input validation
    let sanitized_title = container
        .security_context()
        .input_validator()
        .validate_search_query(&title)
        .map_err(|e| AppError::Other(format!("Invalid title: {}", e)))?;

    // 2. Delegate to service
    let service = container.conversation_service();
    let conversation = service
        .create_conversation(sanitized_title, system_prompt)
        .await?;

    Ok(conversation)
}

/// Tauri command wrapper
#[tracing::instrument(skip(container), fields(title = %title))]
pub async fn create_conversation(
    container: State<'_, Container>,
    title: String,
    system_prompt: Option<String>,
) -> Result<Conversation> {
    create_conversation_impl(container.inner(), title, system_prompt).await
}

/// Lists conversations, most recently updated first, without their messages.
/// `limit` defaults to, and is capped at, 100.
///
/// # Errors
///
/// * `AppError::Other` - Database query failed
pub async fn list_conversations_impl(
    runtime: &dyn ChatRuntime,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<Conversation>> {
    // 1. Validate and cap limit
    let limit = limit.map(|l| l.min(100)); // Cap at 100

    // 2. Delegate to service
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

/// One conversation's metadata by id, without its messages; `None` when not
/// found.
///
/// # Errors
///
/// * `AppError::Other` - Database query failed
pub async fn get_conversation_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
) -> Result<Option<Conversation>> {
    let service = runtime.conversation_service();
    let aggregate = service.get_conversation(&conversation_id).await?;

    // 3. Convert aggregate to conversation (return just metadata, not messages)
    Ok(aggregate.map(|agg| Conversation {
        id: agg.id().clone(),
        title: agg.title().to_string(),
        model_choice: agg.model_choice().clone(),
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
