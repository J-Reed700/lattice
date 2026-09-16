//! Conversation Plugin - Thin Tauri wrappers over conversation command implementations.

use crate::features::conversation::branching_dto::{
    ForkConversationRequestDto, ForkConversationResponseDto, TruncateConversationAfterRequestDto,
    TruncateConversationAfterResponseDto,
};
use crate::features::conversation::chat::{ChatResponse, ToolPreferences};
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
use crate::features::conversation::plugin_impl as conversation_impl;
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
use tauri::{
    plugin::{Builder, TauriPlugin},
    State,
};

pub use crate::features::conversation::plugin_impl::{
    ConversationLinkedDocumentDto, ConversationWebSourceDto, DocumentSpaceMembershipDto,
    SynthesizeJournalEntriesRequestDto, SynthesizeJournalEntriesResponseDto,
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
    request_id: Option<String>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    conversation_impl::chat_with_conversation_wrapper_impl(
        container.inner(),
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        request_id,
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
    request_id: Option<String>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    conversation_impl::chat_with_conversation_impl(
        container.inner(),
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        request_id,
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
    conversation_impl::create_conversation_space(request, container.inner()).await
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
pub async fn create_journal(
    request: CreateConversationJournalRequestDto,
    container: State<'_, Container>,
) -> Result<ConversationJournalDto, ApiError> {
    conversation_impl::create_journal_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_journals(
    container: State<'_, Container>,
) -> Result<Vec<ConversationJournalDto>, ApiError> {
    conversation_impl::list_journals_impl(container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn update_journal(
    request: UpdateConversationJournalRequestDto,
    container: State<'_, Container>,
) -> Result<ConversationJournalDto, ApiError> {
    conversation_impl::update_journal_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn archive_journal(
    request: ArchiveConversationJournalRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::archive_journal_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_journal(
    request: DeleteConversationJournalRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::delete_journal_impl(request, container.inner()).await
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
pub async fn add_conversation_to_journal(
    request: AddConversationToJournalRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::add_conversation_to_journal_impl(request, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_conversation_from_journal(
    request: RemoveConversationFromJournalRequestDto,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::remove_conversation_from_journal_impl(request, container.inner()).await
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
pub async fn add_conversation_web_source(
    conversation_id: String,
    url: String,
    title: Option<String>,
    excerpt: Option<String>,
    relevance_score: Option<f32>,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::add_conversation_web_source_impl(
        conversation_id,
        url,
        title,
        excerpt,
        relevance_score,
        container.inner(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn list_conversation_web_sources(
    conversation_id: String,
    container: State<'_, Container>,
) -> Result<Vec<ConversationWebSourceDto>, ApiError> {
    conversation_impl::list_conversation_web_sources_impl(conversation_id, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_conversation_web_source(
    conversation_id: String,
    source_id: String,
    container: State<'_, Container>,
) -> Result<RenameConversationResponseDto, ApiError> {
    conversation_impl::remove_conversation_web_source_impl(
        conversation_id,
        source_id,
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

#[tauri::command]
#[specta::specta]
pub async fn list_journal_conversations(
    query: ListJournalConversationsQueryDto,
    container: State<'_, Container>,
) -> Result<ListConversationsResponseDto, ApiError> {
    conversation_impl::list_journal_conversations_impl(query, container.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn synthesize_journal_entries(
    request: SynthesizeJournalEntriesRequestDto,
    container: State<'_, Container>,
    window: tauri::Window,
) -> Result<SynthesizeJournalEntriesResponseDto, ApiError> {
    conversation_impl::synthesize_journal_entries_impl(request, container.inner(), window).await
}

/// Delete every message after `message_id` (and it too when `inclusive`),
/// returning what remains.
#[tauri::command]
#[specta::specta]
pub async fn truncate_conversation_after(
    request: TruncateConversationAfterRequestDto,
    container: State<'_, Container>,
) -> Result<TruncateConversationAfterResponseDto, ApiError> {
    conversation_impl::truncate_conversation_after_impl(request, container.inner()).await
}

/// Create a sibling conversation carrying the messages up to a chosen turn.
#[tauri::command]
#[specta::specta]
pub async fn fork_conversation(
    request: ForkConversationRequestDto,
    container: State<'_, Container>,
) -> Result<ForkConversationResponseDto, ApiError> {
    conversation_impl::fork_conversation_impl(request, container.inner()).await
}

/// Re-run the last user message, streaming over `llm-stream` exactly like a
/// normal send. The user message is not duplicated.
///
/// Flat args, mirroring `chat_with_conversation`, because this re-enters the
/// same streaming flow and needs the same `window`.
#[tauri::command]
#[specta::specta]
pub async fn regenerate_response(
    container: State<'_, Container>,
    conversation_id: String,
    tool_preferences: Option<ToolPreferences>,
    request_id: Option<String>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    conversation_impl::regenerate_response_impl(
        container.inner(),
        conversation_id,
        tool_preferences,
        request_id,
        window,
    )
    .await
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
            create_journal,
            list_journals,
            list_conversation_space_members,
            upsert_conversation_space_member,
            remove_conversation_space_member,
            update_conversation_space,
            archive_conversation_space,
            update_journal,
            archive_journal,
            delete_journal,
            move_conversation_to_space,
            add_conversation_to_journal,
            remove_conversation_from_journal,
            set_conversation_saved,
            set_conversation_bookmarked,
            set_conversation_pinned,
            set_conversation_archived,
            list_conversation_linked_documents,
            remove_conversation_linked_document,
            add_conversation_web_source,
            list_conversation_web_sources,
            remove_conversation_web_source,
            list_document_space_memberships,
            set_document_space_membership,
            set_documents_space_membership,
            bookmark_conversation_message,
            unbookmark_conversation_message,
            delete_conversation_message,
            list_message_bookmarks,
            list_conversations_explorer,
            list_journal_conversations,
            synthesize_journal_entries,
            truncate_conversation_after,
            fork_conversation,
            regenerate_response,
        ])
        .build()
}
