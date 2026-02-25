//! Conversation Plugin - Thin Tauri wrappers over conversation command implementations.

use crate::application::dtos::conversation_dto::{
    CreateConversationRequestDto, CreateConversationResponseDto, DeleteConversationRequestDto,
    DeleteConversationResponseDto, GetConversationMessagesRequestDto,
    GetConversationMessagesResponseDto, GetConversationRequestDto, GetConversationResponseDto,
    ListConversationsQuery, ListConversationsResponseDto, RenameConversationRequestDto,
    RenameConversationResponseDto,
};
use crate::application::dtos::conversation_message_bookmark_dto::{
    BookmarkConversationMessageRequestDto, DeleteConversationMessageRequestDto,
    ListMessageBookmarksQueryDto, ListMessageBookmarksResponseDto,
    UnbookmarkConversationMessageRequestDto,
};
use crate::application::dtos::conversation_space_dto::{
    ArchiveConversationSpaceRequestDto, ConversationSpaceDto, ConversationSpaceMemberDto,
    CreateConversationSpaceRequestDto, ListConversationsExplorerQueryDto,
    MoveConversationToSpaceRequestDto, RemoveConversationSpaceMemberRequestDto,
    SetConversationStateRequestDto, UpdateConversationSpaceRequestDto,
    UpsertConversationSpaceMemberRequestDto,
};
use crate::interfaces::commands::conversation_chat::{ChatResponse, ToolPreferences};
use crate::interfaces::commands::conversation_plugin_impl as conversation_impl;
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    State,
};

pub use crate::interfaces::commands::conversation_plugin_impl::{
    ConversationLinkedDocumentDto, DocumentSpaceMembershipDto,
};

#[tauri::command]
#[specta::specta]
pub async fn create_conversation(
    request: CreateConversationRequestDto,
    container: State<'_, Container>,
) -> Result<CreateConversationResponseDto, ApiError> {
    conversation_impl::create_conversation_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_conversation(
    request: GetConversationRequestDto,
    container: State<'_, Container>,
) -> Result<GetConversationResponseDto, ApiError> {
    conversation_impl::get_conversation_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_conversations(
    query: ListConversationsQuery,
    container: State<'_, Container>,
) -> Result<ListConversationsResponseDto, ApiError> {
    conversation_impl::list_conversations_impl(query, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_conversation(
    request: DeleteConversationRequestDto,
    container: State<'_, Container>,
) -> Result<DeleteConversationResponseDto, ApiError> {
    conversation_impl::delete_conversation_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_conversation_messages(
    request: GetConversationMessagesRequestDto,
    container: State<'_, Container>,
) -> Result<GetConversationMessagesResponseDto, ApiError> {
    conversation_impl::get_conversation_messages_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn rename_conversation(
    request: RenameConversationRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::rename_conversation_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn chat_with_conversation_wrapper(
    container: State<'_, Container>,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    conversation_impl::chat_with_conversation_wrapper_impl(
        container.inner(),
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        window,
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn chat_with_conversation(
    container: State<'_, Container>,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    conversation_impl::chat_with_conversation_impl(
        container.inner(),
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        window,
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn create_conversation_space(
    request: CreateConversationSpaceRequestDto,
    container: State<'_, Container>,
) -> Result<ConversationSpaceDto, ApiError> {
    conversation_impl::create_conversation_space_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_conversation_spaces(
    container: State<'_, Container>,
) -> Result<Vec<ConversationSpaceDto>, ApiError> {
    conversation_impl::list_conversation_spaces_impl(container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_conversation_space_members(
    space_id: String,
    container: State<'_, Container>,
) -> Result<Vec<ConversationSpaceMemberDto>, ApiError> {
    conversation_impl::list_conversation_space_members_impl(space_id, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn upsert_conversation_space_member(
    request: UpsertConversationSpaceMemberRequestDto,
    container: State<'_, Container>,
) -> Result<ConversationSpaceMemberDto, ApiError> {
    conversation_impl::upsert_conversation_space_member_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_conversation_space_member(
    request: RemoveConversationSpaceMemberRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::remove_conversation_space_member_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn update_conversation_space(
    request: UpdateConversationSpaceRequestDto,
    container: State<'_, Container>,
) -> Result<ConversationSpaceDto, ApiError> {
    conversation_impl::update_conversation_space_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn archive_conversation_space(
    request: ArchiveConversationSpaceRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::archive_conversation_space_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn move_conversation_to_space(
    request: MoveConversationToSpaceRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::move_conversation_to_space_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_conversation_linked_documents(
    conversation_id: String,
    container: State<'_, Container>,
) -> Result<Vec<ConversationLinkedDocumentDto>, ApiError> {
    conversation_impl::list_conversation_linked_documents_impl(conversation_id, container.inner())
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_conversation_linked_document(
    conversation_id: String,
    document_id: String,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::remove_conversation_linked_document_impl(
        conversation_id,
        document_id,
        container.inner(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn list_document_space_memberships(
    document_id: String,
    container: State<'_, Container>,
) -> Result<Vec<DocumentSpaceMembershipDto>, ApiError> {
    conversation_impl::list_document_space_memberships_impl(document_id, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_document_space_membership(
    document_id: String,
    space_id: String,
    assigned: bool,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::set_document_space_membership_impl(
        document_id,
        space_id,
        assigned,
        container.inner(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_documents_space_membership(
    document_ids: Vec<String>,
    space_id: String,
    assigned: bool,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::set_documents_space_membership_impl(
        document_ids,
        space_id,
        assigned,
        container.inner(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_conversation_saved(
    request: SetConversationStateRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::set_conversation_saved_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_conversation_bookmarked(
    request: SetConversationStateRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::set_conversation_bookmarked_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_conversation_pinned(
    request: SetConversationStateRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::set_conversation_pinned_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_conversation_archived(
    request: SetConversationStateRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::set_conversation_archived_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn bookmark_conversation_message(
    request: BookmarkConversationMessageRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::bookmark_conversation_message_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn unbookmark_conversation_message(
    request: UnbookmarkConversationMessageRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::unbookmark_conversation_message_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_conversation_message(
    request: DeleteConversationMessageRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::delete_conversation_message_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_message_bookmarks(
    query: ListMessageBookmarksQueryDto,
    container: State<'_, Container>,
) -> Result<ListMessageBookmarksResponseDto, ApiError> {
    conversation_impl::list_message_bookmarks_impl(query, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_conversations_explorer(
    query: ListConversationsExplorerQueryDto,
    container: State<'_, Container>,
) -> Result<ListConversationsResponseDto, ApiError> {
    conversation_impl::list_conversations_explorer_impl(query, container.inner()).await
}

pub fn init() -> TauriPlugin<tauri::Wry> {
    Builder::new("conversation")
        .invoke_handler(tauri::generate_handler![
            create_conversation,
            get_conversation,
            list_conversations,
            delete_conversation,
            get_conversation_messages,
            rename_conversation,
            chat_with_conversation_wrapper,
            chat_with_conversation,
            create_conversation_space,
            list_conversation_spaces,
            list_conversation_space_members,
            upsert_conversation_space_member,
            remove_conversation_space_member,
            update_conversation_space,
            archive_conversation_space,
            move_conversation_to_space,
            set_conversation_saved,
            set_conversation_bookmarked,
            set_conversation_pinned,
            set_conversation_archived,
            list_conversation_linked_documents,
            remove_conversation_linked_document,
            list_document_space_memberships,
            set_document_space_membership,
            set_documents_space_membership,
            bookmark_conversation_message,
            unbookmark_conversation_message,
            delete_conversation_message,
            list_message_bookmarks,
            list_conversations_explorer,
        ])
        .build()
}
