//! The documents and web sources linked to a conversation or space.

use crate::features::conversation::dto::RenameConversationResponseDto;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::workspace_dto::{
    ConversationLinkedDocumentDto, ConversationWebSourceDto, SpaceDocumentDto,
};
use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;

/// File a chat's attachments in the library, where they become ordinary
/// documents.
///
/// The one way out of conversation scope. Until it is called, an attached file
/// is the owning chat's alone; afterwards it is listed in the library, searched
/// with the rest of the vault, and outlives the conversation it arrived in.
///
/// Documents that were never a chat's attachment are untouched, so calling this
/// twice is harmless.
pub async fn add_documents_to_library_impl(
    document_ids: Vec<String>,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let filed = document_ids.len();
    container
        .document_scope()
        .set_conversation_owner(&document_ids, None)
        .await
        .map_err(ApiError::from)?;
    tracing::info!(filed, "Filed a conversation's attachments in the library");
    Ok(RenameConversationResponseDto {
        status: "added".to_string(),
    })
}

/// The documents a chat may name, for the composer's `@` picker.
///
/// `space_id` of `None` or blank means General. The list comes from the same
/// scope retrieval derives its allow-list from, so what the picker offers is
/// exactly what the turn can read — including, when `conversation_id` is given,
/// the files attached to that chat.
pub async fn list_space_documents_impl(
    space_id: Option<String>,
    conversation_id: Option<String>,
    query: Option<String>,
    limit: Option<u32>,
    container: &Container,
) -> Result<Vec<SpaceDocumentDto>, ApiError> {
    ConversationRepository::new(container.db_pool().clone())
        .space_documents(
            space_id.as_deref(),
            conversation_id.as_deref(),
            query.as_deref().unwrap_or_default(),
            limit.unwrap_or(8) as usize,
        )
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

/// Unlink a document from a conversation.
///
/// For a library document this only drops the link — the document itself is
/// filed elsewhere and stays. For a file attached to *this* chat there is no
/// elsewhere: unlinking it would leave a document nothing lists, nothing
/// searches and nothing can delete, still feeding this chat's retrieval. So
/// "Remove" means what it says and deletes it, chunks, vectors and blob.
pub async fn remove_conversation_linked_document_impl(
    conversation_id: String,
    document_id: String,
    container: &Container,
) -> Result<RenameConversationResponseDto, ApiError> {
    let owned = container
        .document_scope()
        .documents_owned_by_conversation(&conversation_id)
        .await
        .map_err(ApiError::from)?
        .contains(&document_id);

    if owned {
        // The document goes first and the link cascades with it. Doing it the
        // other way round would, on a failed delete, leave a document owned by
        // a live conversation that nothing links to: invisible everywhere, and
        // missed by the startup sweep, which only collects the ones whose
        // conversation is gone. A failure here surfaces to the user with both
        // the file and its link still in place, so they can try again.
        container
            .delete_document_use_case()
            .execute(document_id.clone())
            .await
            .map_err(ApiError::from)?;
        tracing::info!(conversation_id, document_id, "Deleted an attachment");
        return Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        });
    }

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
