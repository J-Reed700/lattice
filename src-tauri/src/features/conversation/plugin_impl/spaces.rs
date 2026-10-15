//! Spaces and journals, and moving a conversation between spaces.

use crate::features::conversation::commands as conversation;
use crate::features::conversation::dto::{
    ListConversationsResponseDto, RenameConversationResponseDto,
};
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::space_dto::{
    AddConversationToJournalRequestDto, ArchiveConversationJournalRequestDto,
    ArchiveConversationSpaceRequestDto, ConversationJournalDto, ConversationSpaceDto,
    ConversationSpaceMemberDto, CreateConversationJournalRequestDto,
    CreateConversationSpaceRequestDto, DeleteConversationJournalRequestDto,
    ListJournalConversationsQueryDto, MoveConversationToSpaceRequestDto,
    RemoveConversationFromJournalRequestDto, RemoveConversationSpaceMemberRequestDto,
    SetJournalEntryPinnedRequestDto, UpdateConversationJournalRequestDto,
    UpdateConversationSpaceRequestDto, UpsertConversationSpaceMemberRequestDto,
};
use crate::features::conversation::workspace_dto::DocumentSpaceMembershipDto;
use crate::interfaces::di::Container;
use crate::shared::ipc::ApiError;

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

pub async fn set_journal_entry_pinned_impl(
    request: SetJournalEntryPinnedRequestDto,
    container: &Container,
) -> Result<(), ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .set_journal_entry_pinned(request)
        .await
        .map_err(ApiError::from)
}

pub async fn list_journal_entry_pins_impl(
    journal_space_id: String,
    container: &Container,
) -> Result<Vec<String>, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .list_journal_entry_pins(&journal_space_id)
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
