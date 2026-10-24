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
///
/// # Returns
///
/// * `Ok(Vec<TagDto>)` - List of all tags with metadata
/// * `Err(AppError)` - If query fails (database error)
///
/// # Example
///
/// ```typescript
/// const tags = await invoke<Array<{
///   id: string;
///   name: string;
///   color?: string;
///   description?: string;
/// }>>('get_all_tags_ddd');
///
/// console.log(`Total tags: ${tags.length}`);
/// tags.forEach(tag => console.log(`- ${tag.name}`));
/// ```
///
/// # DDD Architecture
///
/// Simple query command - delegates to `GetTagsUseCase`.
pub async fn get_all_tags_ddd(container: State<'_, Container>) -> Result<Vec<TagDto>> {
    let use_case = container.get_tags_use_case();
    let tags = use_case.execute().await?;

    Ok(tags)
}

/// Retrieves all tags with document counts
///
/// Returns tags with the count of documents each tag is applied to. Useful for
/// displaying tag popularity or filtering empty tags.
///
/// # Returns
///
/// * `Ok(Vec<TagWithCountDto>)` - Tags with document counts
/// * `Err(AppError)` - If query fails
///
/// # Example
///
/// ```typescript
/// const tagsWithCounts = await invoke<Array<{
///   tag: { id: string; name: string };
///   documentCount: number;
/// }>>('get_all_tags_with_counts_ddd');
///
/// tagsWithCounts.forEach(item => {
///   console.log(`${item.tag.name}: ${item.documentCount} documents`);
/// });
/// ```
///
/// # DDD Architecture
///
/// Query command with aggregate - delegates to `GetTagsUseCase.get_with_counts()`.
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn get_all_tags_with_counts_impl(container: &Container) -> Result<Vec<TagWithCountDto>> {
    let use_case = container.get_tags_use_case();
    let tags = use_case.get_with_counts().await?;

    Ok(tags)
}

/// ## Tauri Command Layer (Thin Wrapper)
pub async fn get_all_tags_with_counts_ddd(
    container: State<'_, Container>,
) -> Result<Vec<TagWithCountDto>> {
    get_all_tags_with_counts_impl(container.inner()).await
}

/// Retrieves all tags applied to a specific document
///
/// Returns the list of tags currently applied to the specified document.
/// Useful for displaying document tags and managing tag assignments.
///
/// # Arguments
///
/// * `document_id` - Document identifier
///
/// # Returns
///
/// * `Ok(Vec<TagDto>)` - List of tags applied to document
/// * `Err(AppError)` - If document not found or query fails
///
/// # Example
///
/// ```typescript
/// const documentTags = await invoke<Array<{
///   id: string;
///   name: string;
///   color?: string;
/// }>>('get_document_tags_ddd', {
///   documentId: 'doc_123'
/// });
///
/// console.log(`Document has ${documentTags.length} tags`);
/// ```
///
/// # DDD Architecture
///
/// Query command - delegates to `GetTagsUseCase.get_for_document()`.
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn get_document_tags_impl(
    container: &Container,
    document_id: String,
) -> Result<Vec<TagDto>> {
    let use_case = container.get_tags_use_case();
    let tags = use_case.get_for_document(document_id).await?;

    Ok(tags)
}

/// ## Tauri Command Layer (Thin Wrapper)
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
///
/// # Arguments
///
/// * `request` - Search request with tag name and optional filters
///
/// # Returns
///
/// * `Ok(SearchByTagResponseDto)` - Matching documents with metadata
/// * `Err(AppError)` - If tag not found or search fails
///
/// # Example
///
/// ```typescript
/// const results = await invoke<{
///   documents: Array<{
///     id: string;
///     filePath: string;
///     title: string;
///   }>;
///   total: number;
/// }>('search_by_tag_ddd', {
///   request: {
///     tagName: 'Research',
///     limit: 50,
///     offset: 0
///   }
/// });
///
/// console.log(`Found ${results.total} documents with tag "Research"`);
/// ```
///
/// # DDD Architecture
///
/// Query command - delegates to `SearchByTagUseCase`.
pub async fn search_by_tag_ddd(
    container: State<'_, Container>,
    request: SearchByTagRequestDto,
) -> Result<SearchByTagResponseDto> {
    let use_case = container.search_by_tag_use_case();
    let response = use_case.execute(request).await?;

    Ok(response)
}
