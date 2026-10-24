//! Tag Command Handlers
//!
//! Handlers for document tagging operations.
//! Provides CRUD operations, query commands, tag assignment, and LLM-based tag generation.
//! These commands apply cross-cutting concerns (rate limiting, validation)
//! and delegate business logic to dedicated use cases.

use crate::features::tags::dto::*;
use crate::interfaces::di::Container;
use lattice_core::application::contracts::tags::TagWithCountDto;
use lattice_core::shared::error::Result;
use tauri::State;

mod assignment;
mod crud;
mod generation;
mod queries;

pub use assignment::{
    apply_tags_ddd, apply_tags_impl, remove_tag_from_document_ddd, remove_tag_from_document_impl,
};
pub use crud::{create_tag_ddd, delete_tag_ddd, update_tag_ddd};
pub use generation::{auto_tag_all_documents_ddd, generate_tags_ddd, generate_tags_impl};
pub use queries::{
    get_all_tags_ddd, get_all_tags_with_counts_ddd, get_all_tags_with_counts_impl,
    get_document_tags_ddd, get_document_tags_impl, search_by_tag_ddd,
};

/// Legacy: Apply tags to a document (compatibility wrapper)
pub async fn apply_tags(
    document_id: String,
    tags: Vec<String>,
    container: State<'_, Container>,
) -> Result<Vec<TagDto>> {
    let request = ApplyTagsRequestDto {
        document_id,
        tag_names: tags,
    };

    let response = apply_tags_ddd(container, request).await?;
    Ok(response.tags)
}

/// Legacy: Get document tags (compatibility wrapper)
pub async fn get_document_tags(
    document_id: String,
    container: State<'_, Container>,
) -> Result<Vec<TagDto>> {
    get_document_tags_ddd(container, document_id).await
}

/// Legacy: Generate tags (compatibility wrapper)
pub async fn generate_tags(
    document_id: String,
    container: State<'_, Container>,
) -> Result<Vec<String>> {
    let request = GenerateTagsRequestDto {
        document_id,
        max_tags: 5,
    };

    let response = generate_tags_ddd(container, request).await?;
    Ok(response.tags)
}

/// Legacy: Get all tags (compatibility wrapper)
pub async fn get_all_tags(container: State<'_, Container>) -> Result<Vec<TagDto>> {
    get_all_tags_ddd(container).await
}

/// Legacy: Get all tags with counts (compatibility wrapper)
pub async fn get_all_tags_with_counts(
    container: State<'_, Container>,
) -> Result<Vec<TagWithCountDto>> {
    get_all_tags_with_counts_ddd(container).await
}

/// Legacy: Search by tag (compatibility wrapper)
pub async fn search_by_tag(
    tag_name: String,
    container: State<'_, Container>,
) -> Result<Vec<String>> {
    let request = SearchByTagRequestDto { tag_name };
    let response = search_by_tag_ddd(container, request).await?;
    Ok(response.document_ids)
}

/// Legacy: Create tag (compatibility wrapper)
pub async fn create_tag(
    name: String,
    color: Option<String>,
    container: State<'_, Container>,
) -> Result<TagDto> {
    let request = CreateTagRequestDto {
        name,
        color,
        description: None,
    };

    let response = create_tag_ddd(container, request).await?;
    Ok(response.tag)
}

/// Legacy: Update tag (compatibility wrapper)
pub async fn update_tag(
    tag_id: String,
    name: Option<String>,
    color: Option<String>,
    container: State<'_, Container>,
) -> Result<TagDto> {
    let request = UpdateTagRequestDto {
        id: tag_id,
        name,
        color,
        description: None,
    };

    update_tag_ddd(container, request).await
}

/// Legacy: Delete tag (compatibility wrapper)
pub async fn delete_tag(tag_id: String, container: State<'_, Container>) -> Result<()> {
    let request = DeleteTagRequestDto { tag_id };
    delete_tag_ddd(container, request).await
}

/// Legacy: Remove tag from document (compatibility wrapper)
pub async fn remove_tag_from_document(
    document_id: String,
    tag_id: String,
    container: State<'_, Container>,
) -> Result<()> {
    let request = RemoveTagRequestDto {
        document_id,
        tag_id,
    };

    remove_tag_from_document_ddd(container, request).await
}

/// Legacy: Auto-tag all documents (compatibility wrapper)
pub async fn auto_tag_all_documents(container: State<'_, Container>) -> Result<usize> {
    let request = AutoTagRequestDto { max_documents: 100 };

    let response = auto_tag_all_documents_ddd(container, request).await?;
    Ok(response.tagged_count)
}
