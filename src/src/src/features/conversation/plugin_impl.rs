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
    AddConversationToJournalRequestDto, ArchiveConversationJournalRequestDto,
    ArchiveConversationSpaceRequestDto, ConversationJournalDto, ConversationSpaceDto,
    ConversationSpaceMemberDto, CreateConversationJournalRequestDto,
    CreateConversationSpaceRequestDto, DeleteConversationJournalRequestDto,
    ListConversationsExplorerQueryDto, ListJournalConversationsQueryDto,
    MoveConversationToSpaceRequestDto, RemoveConversationFromJournalRequestDto,
    RemoveConversationSpaceMemberRequestDto, SetConversationStateRequestDto,
    UpdateConversationJournalRequestDto, UpdateConversationSpaceRequestDto,
    UpsertConversationSpaceMemberRequestDto,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;

pub use super::branching::{
    fork_conversation_impl, regenerate_response_impl, truncate_conversation_after_impl,
};
pub use super::synthesis::synthesize_journal_entries_impl;
pub use super::workspace_dto::*;

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
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    chat_with_conversation_wrapper_impl(
        container,
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        request_id,
        window,
    )
    .await
}

pub async fn create_conversation_space(
    request: CreateConversationSpaceRequestDto,
    container: &Container,
) -> Result<ConversationSpaceDto, ApiError> {
    conversation::create_conversation_space_impl(container, request)
        .await
        .map_err(ApiError::from)
}

pub async fn list_conversation_spaces_impl(
    container: &Container,
) -> Result<Vec<ConversationSpaceDto>, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_conversation_spaces()
        .await
        .map_err(ApiError::from)
}

pub async fn update_conversation_space_impl(
    request: UpdateConversationSpaceRequestDto,
    container: &Container,
) -> Result<ConversationSpaceDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .update_conversation_space(request)
        .await
        .map_err(ApiError::from)
}

pub async fn archive_conversation_space_impl(
    request: ArchiveConversationSpaceRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .archive_conversation_space(request)
        .await
        .map_err(ApiError::from)
}

pub async fn move_conversation_to_space_impl(
    request: MoveConversationToSpaceRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .move_conversation_to_space(request)
        .await
        .map_err(ApiError::from)
}

pub async fn create_journal_impl(
    request: CreateConversationJournalRequestDto,
    container: &Container,
) -> Result<ConversationJournalDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .create_journal(request)
        .await
        .map_err(ApiError::from)
}

pub async fn list_journals_impl(
    container: &Container,
) -> Result<Vec<ConversationJournalDto>, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_journals()
        .await
        .map_err(ApiError::from)
}

pub async fn update_journal_impl(
    request: UpdateConversationJournalRequestDto,
    container: &Container,
) -> Result<ConversationJournalDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .update_journal(request)
        .await
        .map_err(ApiError::from)
}

pub async fn archive_journal_impl(
    request: ArchiveConversationJournalRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .archive_journal(request)
        .await
        .map_err(ApiError::from)
}

pub async fn delete_journal_impl(
    request: DeleteConversationJournalRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .delete_journal(request)
        .await
        .map_err(ApiError::from)
}

pub async fn add_conversation_to_journal_impl(
    request: AddConversationToJournalRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .add_conversation_to_journal(request)
        .await
        .map_err(ApiError::from)
}

pub async fn remove_conversation_from_journal_impl(
    request: RemoveConversationFromJournalRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .remove_conversation_from_journal(request)
        .await
        .map_err(ApiError::from)
}

pub async fn list_journal_conversations_impl(
    query: ListJournalConversationsQueryDto,
    container: &Container,
) -> Result<ListConversationsResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_journal_conversations(query)
        .await
        .map_err(ApiError::from)
}

pub async fn list_conversation_space_members_impl(
    space_id: String,
    container: &Container,
) -> Result<Vec<ConversationSpaceMemberDto>, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_conversation_space_members(space_id)
        .await
        .map_err(ApiError::from)
}

pub async fn upsert_conversation_space_member_impl(
    request: UpsertConversationSpaceMemberRequestDto,
    container: &Container,
) -> Result<ConversationSpaceMemberDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .upsert_conversation_space_member(request)
        .await
        .map_err(ApiError::from)
}

pub async fn remove_conversation_space_member_impl(
    request: RemoveConversationSpaceMemberRequestDto,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .remove_conversation_space_member(request)
        .await
        .map_err(ApiError::from)
}

pub async fn list_document_space_memberships_impl(
    document_id: String,
    container: &Container,
) -> Result<Vec<DocumentSpaceMembershipDto>, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_document_space_memberships(document_id)
        .await
        .map_err(ApiError::from)
}

pub async fn set_document_space_membership_impl(
    document_id: String,
    space_id: String,
    assigned: bool,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .set_document_space_membership(document_id, space_id, assigned)
        .await
        .map_err(ApiError::from)
}

pub async fn set_documents_space_membership_impl(
    document_ids: Vec<String>,
    space_id: String,
    assigned: bool,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .set_documents_space_membership(document_ids, space_id, assigned)
        .await
        .map_err(ApiError::from)
}

pub async fn list_conversation_linked_documents_impl(
    conversation_id: String,
    container: &Container,
) -> Result<Vec<ConversationLinkedDocumentDto>, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_conversation_linked_documents(conversation_id)
        .await
        .map_err(ApiError::from)
}

pub async fn remove_conversation_linked_document_impl(
    conversation_id: String,
    document_id: String,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .remove_conversation_linked_document(conversation_id, document_id)
        .await
        .map_err(ApiError::from)
}

pub async fn add_conversation_web_source_impl(
    conversation_id: String,
    url: String,
    title: Option<String>,
    excerpt: Option<String>,
    relevance_score: Option<f32>,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .add_conversation_web_source(conversation_id, url, title, excerpt, relevance_score)
        .await
        .map_err(ApiError::from)
}

pub async fn list_conversation_web_sources_impl(
    conversation_id: String,
    container: &Container,
) -> Result<Vec<ConversationWebSourceDto>, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_conversation_web_sources(conversation_id)
        .await
        .map_err(ApiError::from)
}

pub async fn remove_conversation_web_source_impl(
    conversation_id: String,
    source_id: String,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .remove_conversation_web_source(conversation_id, source_id)
        .await
        .map_err(ApiError::from)
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
