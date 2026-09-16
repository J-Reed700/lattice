//! Conversation branching and regeneration workflows.
use super::branching_dto::*;
use super::chat::{
    chat_with_conversation_impl as run_chat_with_conversation_impl, ChatResponse, ToolPreferences,
};
use crate::features::conversation::repository::ConversationRepository;
use crate::interfaces::di::Container;
use crate::shared::{api_result::ApiError, error::AppError};
fn to_message_dto(
    message: &crate::domain::conversation::ConversationMessage,
) -> crate::features::conversation::dto::MessageDto {
    crate::features::conversation::dto::MessageDto {
        id: message.id.to_string(),
        conversation_id: message.conversation_id.to_string(),
        role: message.role.to_string(),
        content: message.content.clone(),
        tokens: message.tokens,
        created_at: message.created_at.to_rfc3339(),
        metadata: message.metadata.clone(),
        status: message.status.clone(),
    }
}

pub async fn truncate_conversation_after_impl(
    request: TruncateConversationAfterRequestDto,
    container: &Container,
) -> Result<TruncateConversationAfterResponseDto, ApiError> {
    let repo = ConversationRepository::new(container.db_pool().clone());

    let deleted = repo
        .truncate_after(
            &request.conversation_id,
            &request.message_id,
            request.inclusive,
        )
        .await
        .map_err(ApiError::from)?;

    let messages = repo
        .get_messages(&request.conversation_id)
        .await
        .map_err(ApiError::from)?;

    Ok(TruncateConversationAfterResponseDto {
        conversation_id: request.conversation_id,
        deleted_count: deleted as u32,
        messages: messages.iter().map(to_message_dto).collect(),
    })
}

pub async fn fork_conversation_impl(
    request: ForkConversationRequestDto,
    container: &Container,
) -> Result<ForkConversationResponseDto, ApiError> {
    let repo = ConversationRepository::new(container.db_pool().clone());

    let source = repo
        .find_by_id(&request.conversation_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::from(AppError::NotFound(format!(
                "Conversation {} not found",
                request.conversation_id
            )))
        })?;

    let mut new_title = format!("{} · branch", source.title);
    if new_title.chars().count() > 200 {
        new_title = new_title.chars().take(200).collect();
    }
    let new_id = uuid::Uuid::new_v4().to_string();

    let (new_id, copied) = repo
        .fork(
            &request.conversation_id,
            request.up_to_message_id.as_deref(),
            &new_id,
            &new_title,
        )
        .await
        .map_err(ApiError::from)?;

    let created = repo
        .find_by_id(&new_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::from(AppError::InvalidState(
                "The branch was created but could not be read back.".to_string(),
            ))
        })?;

    Ok(ForkConversationResponseDto {
        conversation: ConversationRepository::new(container.db_pool().clone())
            .project_conversation(&created)
            .await
            .map_err(ApiError::from)?,
        copied_message_count: copied,
    })
}

pub async fn regenerate_response_impl(
    container: &Container,
    conversation_id: String,
    tool_preferences: Option<ToolPreferences>,
    request_id: Option<String>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    let repo = ConversationRepository::new(container.db_pool().clone());

    let (content, _tokens) = repo
        .take_last_user_turn(&conversation_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::from(AppError::InvalidInput(
                "This conversation has nothing to regenerate.".to_string(),
            ))
        })?;

    let fut = run_chat_with_conversation_impl(
        container,
        Some(conversation_id.clone()),
        content.clone(),
        tool_preferences,
        None,
        request_id,
        window,
    );

    match Box::pin(fut).await {
        Ok(response) => Ok(response),
        Err(error) => {
            // Put the question back on the thread before surfacing the failure.
            if let Err(persist_error) = container
                .conversation_service()
                .add_message_with_status(
                    &conversation_id,
                    crate::domain::conversation::MessageRole::User,
                    content,
                    0,
                    "failed".to_string(),
                )
                .await
            {
                tracing::error!(
                    error = %persist_error,
                    conversation_id = conversation_id.as_str(),
                    "Failed to restore the user message after a failed regenerate"
                );
            }
            Err(ApiError::from(error))
        }
    }
}
