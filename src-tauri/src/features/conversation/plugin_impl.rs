//! Conversation command facade. Persistence and workflows live in focused modules.

use crate::features::conversation::chat::{
    chat_with_conversation_impl as run_chat_with_conversation_impl, ChatResponse, ToolPreferences,
};
use crate::features::conversation::commands as conversation;
use crate::features::conversation::dto::{
    CreateConversationRequestDto, CreateConversationResponseDto, DeleteConversationRequestDto,
    DeleteConversationResponseDto, GetConversationMessagesRequestDto,
    GetConversationMessagesResponseDto, GetConversationRequestDto, GetConversationResponseDto,
    ListConversationsQuery, ListConversationsResponseDto, RenameConversationRequestDto,
    RenameConversationResponseDto,
};
use crate::features::conversation::message_bookmark_dto::{
    BookmarkConversationMessageRequestDto, DeleteConversationMessageRequestDto,
    ListMessageBookmarksQueryDto, ListMessageBookmarksResponseDto,
    UnbookmarkConversationMessageRequestDto,
};
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::space_dto::{
    ListConversationsExplorerQueryDto, SetConversationStateRequestDto,
};
use crate::interfaces::di::Container;
use crate::shared::ipc::ApiError;

pub use super::branching::{
    create_conversation_tangent_impl, fork_conversation_impl, list_conversation_tangents_impl,
    promote_conversation_tangent_impl, regenerate_response_impl, truncate_conversation_after_impl,
};
pub use super::handoff::continue_in_new_conversation_impl;
pub use super::synthesis::{
    dismiss_journal_synthesis_impl, get_journal_synthesis_result_impl, list_journal_syntheses_impl,
    mark_journal_synthesis_applied_impl, retry_journal_synthesis_impl,
    synthesize_journal_entries_impl,
};
pub use super::workspace_dto::*;

mod memory;
mod sources;
mod spaces;

pub use memory::{compact_conversation_impl, get_conversation_memory_impl, manage_knowledge_impl};
pub use sources::{
    add_conversation_web_source_impl, add_documents_to_library_impl,
    list_conversation_linked_documents_impl, list_conversation_web_sources_impl,
    list_space_documents_impl, remove_conversation_linked_document_impl,
    remove_conversation_web_source_impl,
};
pub use spaces::{
    add_conversation_to_journal_impl, archive_conversation_space_impl, archive_journal_impl,
    create_conversation_space, create_journal_impl, delete_journal_impl,
    list_conversation_space_members_impl, list_conversation_spaces_impl,
    list_document_space_memberships_impl, list_journal_conversations_impl,
    list_journal_entry_pins_impl, list_journals_impl, move_conversation_to_space_impl,
    remove_conversation_from_journal_impl, remove_conversation_space_member_impl,
    set_document_space_membership_impl, set_documents_space_membership_impl,
    set_journal_entry_pinned_impl, update_conversation_space_impl, update_journal_impl,
    upsert_conversation_space_member_impl,
};

pub async fn create_conversation_impl(
    request: CreateConversationRequestDto,
    container: &Container,
) -> Result<CreateConversationResponseDto, ApiError> {
    let conversation = conversation::create_conversation_impl(
        container,
        request.title,
        request.model_name,
        request.system_prompt,
    )
    .await
    .map_err(ApiError::from)?;

    Ok(CreateConversationResponseDto {
        conversation: ConversationRepository::new(container.db_pool().clone())
            .project_conversation(&conversation)
            .await
            .map_err(ApiError::from)?,
        status: "success".to_string(),
    })
}

pub async fn get_conversation_impl(
    request: GetConversationRequestDto,
    container: &Container,
) -> Result<GetConversationResponseDto, ApiError> {
    let conversation = conversation::get_conversation_impl(container, request.conversation_id)
        .await
        .map_err(ApiError::from)?;

    Ok(GetConversationResponseDto {
        conversation: match conversation {
            Some(c) => Some(
                ConversationRepository::new(container.db_pool().clone())
                    .project_conversation(&c)
                    .await
                    .map_err(ApiError::from)?,
            ),
            None => None,
        },
    })
}

pub async fn list_conversations_impl(
    query: ListConversationsQuery,
    container: &Container,
) -> Result<ListConversationsResponseDto, ApiError> {
    let conversations = conversation::list_conversations_impl(container, query.limit, query.offset)
        .await
        .map_err(ApiError::from)?;

    let mut items = Vec::with_capacity(conversations.len());
    for c in &conversations {
        items.push(
            ConversationRepository::new(container.db_pool().clone())
                .project_conversation(c)
                .await
                .map_err(ApiError::from)?,
        );
    }

    Ok(ListConversationsResponseDto {
        conversations: items,
        total: conversations.len(),
    })
}

pub async fn delete_conversation_impl(
    request: DeleteConversationRequestDto,
    container: &Container,
) -> Result<DeleteConversationResponseDto, ApiError> {
    conversation::delete_conversation_impl(container, request.conversation_id)
        .await
        .map_err(ApiError::from)?;

    Ok(DeleteConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn get_conversation_messages_impl(
    request: GetConversationMessagesRequestDto,
    container: &Container,
) -> Result<GetConversationMessagesResponseDto, ApiError> {
    let messages = conversation::get_conversation_messages_impl(container, request.conversation_id)
        .await
        .map_err(ApiError::from)?;

    Ok(GetConversationMessagesResponseDto {
        messages: messages
            .iter()
            .map(|m| crate::features::conversation::dto::MessageDto {
                id: m.id.to_string(),
                conversation_id: m.conversation_id.to_string(),
                role: m.role.to_string(),
                content: m.content.clone(),
                tokens: m.tokens,
                created_at: m.created_at.to_rfc3339(),
                metadata: m.metadata.clone(),
                status: m.status.clone(),
            })
            .collect(),
        total: messages.len(),
    })
}

pub async fn rename_conversation_impl(
    request: RenameConversationRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation::rename_conversation_impl(container, request.conversation_id, request.new_title)
        .await
        .map_err(ApiError::from)?;

    Ok(RenameConversationResponseDto {
        status: "success".to_string(),
    })
}

pub async fn chat_with_conversation_wrapper_impl(
    container: &Container,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    request_id: Option<String>,
    attachment_names: Option<Vec<String>>,
    attachment_document_ids: Option<Vec<String>>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    let convo_id_for_log = conversation_id.clone().unwrap_or_else(|| "NEW".to_string());
    let message_len = message.len();
    tracing::info!(
        conversation_id = convo_id_for_log.as_str(),
        message_len = message_len,
        "chat_with_conversation_wrapper: START"
    );

    let fut = run_chat_with_conversation_impl(
        container,
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        request_id,
        attachment_names,
        attachment_document_ids,
        window,
    );
    match Box::pin(fut).await {
        Ok(response) => {
            tracing::info!(
                conversation_id = response.conversation_id.as_str(),
                response_len = response.message.len(),
                "chat_with_conversation_wrapper: DONE"
            );
            Ok(response)
        }
        Err(e) => {
            tracing::error!(
                conversation_id = convo_id_for_log.as_str(),
                error = %e,
                "chat_with_conversation_wrapper: FAILED"
            );
            Err(ApiError::from(e))
        }
    }
}

pub async fn chat_with_conversation_impl(
    container: &Container,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    request_id: Option<String>,
    attachment_names: Option<Vec<String>>,
    attachment_document_ids: Option<Vec<String>>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    chat_with_conversation_wrapper_impl(
        container,
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        request_id,
        attachment_names,
        attachment_document_ids,
        window,
    )
    .await
}

pub async fn set_conversation_saved_impl(
    request: SetConversationStateRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .set_conversation_saved(request)
        .await
        .map_err(ApiError::from)
}

pub async fn set_conversation_bookmarked_impl(
    request: SetConversationStateRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .set_conversation_bookmarked(request)
        .await
        .map_err(ApiError::from)
}

pub async fn set_conversation_pinned_impl(
    request: SetConversationStateRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .set_conversation_pinned(request)
        .await
        .map_err(ApiError::from)
}

pub async fn set_conversation_archived_impl(
    request: SetConversationStateRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .set_conversation_archived(request)
        .await
        .map_err(ApiError::from)
}

pub async fn bookmark_conversation_message_impl(
    request: BookmarkConversationMessageRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .bookmark_conversation_message(request)
        .await
        .map_err(ApiError::from)
}

pub async fn unbookmark_conversation_message_impl(
    request: UnbookmarkConversationMessageRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .unbookmark_conversation_message(request)
        .await
        .map_err(ApiError::from)
}

pub async fn delete_conversation_message_impl(
    request: DeleteConversationMessageRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .delete_conversation_message(request)
        .await
        .map_err(ApiError::from)
}

pub async fn list_message_bookmarks_impl(
    query: ListMessageBookmarksQueryDto,
    container: &Container,
) -> Result<ListMessageBookmarksResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_message_bookmarks(query)
        .await
        .map_err(ApiError::from)
}

pub async fn list_conversations_explorer_impl(
    query: ListConversationsExplorerQueryDto,
    container: &Container,
) -> Result<ListConversationsResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_conversations_explorer(query)
        .await
        .map_err(ApiError::from)
}
