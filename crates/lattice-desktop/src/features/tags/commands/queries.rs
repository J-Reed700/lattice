//! Listing tags, a document's tags, and documents by tag.

use tauri::State;

use crate::features::tags::dto::*;
use crate::interfaces::di::Container;
use lattice_core::application::contracts::tags::TagWithCountDto;
use lattice_core::shared::error::Result;

/// Retrieves all tags in the system
///
/// Returns a complete list of all tags, useful for displaying available tags in UI
/// for filtering or tag selection interfaces.
pub async fn get_all_tags_ddd(container: State<'_, Container>) -> Result<Vec<TagDto>> {
    let use_case = container.get_tags_use_case();
    let tags = use_case.execute().await?;

    Ok(tags)
}

/// Retrieves all tags with document counts
///
/// Returns tags with the count of documents each tag is applied to. Useful for
/// displaying tag popularity or filtering empty tags.
pub async fn get_all_tags_with_counts_impl(container: &Container) -> Result<Vec<TagWithCountDto>> {
    let use_case = container.get_tags_use_case();
    let tags = use_case.get_with_counts().await?;

    Ok(tags)
}

pub async fn get_all_tags_with_counts_ddd(
    container: State<'_, Container>,
) -> Result<Vec<TagWithCountDto>> {
    get_all_tags_with_counts_impl(container.inner()).await
}

/// Retrieves all tags applied to a specific document
///
/// Returns the list of tags currently applied to the specified document.
/// Useful for displaying document tags and managing tag assignments.
pub async fn get_document_tags_impl(
    container: &Container,
    document_id: String,
) -> Result<Vec<TagDto>> {
    let use_case = container.get_tags_use_case();
    let tags = use_case.get_for_document(document_id).await?;

    Ok(tags)
}

pub async fn get_document_tags_ddd(
    container: State<'_, Container>,
    document_id: String,
) -> Result<Vec<TagDto>> {
    get_document_tags_impl(container.inner(), document_id).await
}

/// Searches for documents by tag name
///
/// Finds all documents that have the specified tag applied. Supports filtering
/// and pagination for large result sets.
pub async fn search_by_tag_ddd(
    container: State<'_, Container>,
    request: SearchByTagRequestDto,
) -> Result<SearchByTagResponseDto> {
    let use_case = container.search_by_tag_use_case();
    let response = use_case.execute(request).await?;

    Ok(response)
}
