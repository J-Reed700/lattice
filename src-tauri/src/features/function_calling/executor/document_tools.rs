//! `get_document` and `list_documents` tool handlers.

use super::FunctionExecutor;
use crate::features::function_calling::domain::FunctionResult;
use crate::features::function_calling::dto::*;
use crate::shared::error::{AppError, Result};
use std::collections::HashSet;
use tracing::{debug, info};

impl FunctionExecutor {
    fn slice_by_char_range(content: &str, start: usize, end: usize) -> String {
        if start >= end {
            return String::new();
        }

        let mut start_byte = content.len();
        let mut end_byte = content.len();

        for (char_idx, (byte_idx, _)) in content.char_indices().enumerate() {
            if char_idx == start {
                start_byte = byte_idx;
            }
            if char_idx == end {
                end_byte = byte_idx;
                break;
            }
        }

        if start == 0 {
            start_byte = 0;
        }

        if end >= content.chars().count() {
            end_byte = content.len();
        }

        if start_byte >= end_byte || start_byte > content.len() || end_byte > content.len() {
            return String::new();
        }

        content[start_byte..end_byte].to_string()
    }

    /// Execute get_document function
    ///
    /// Boxed to prevent stack overflow: the Vec<Chunk> from find_by_document
    /// would inflate the parent execute() Future state machine.
    pub(super) fn handle_get_document<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: GetDocumentInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!("Get document: id='{}'", input.document_id);

            let doc = self
                .document_repository
                .find_by_id(&input.document_id)
                .await?
                .ok_or_else(|| {
                    AppError::NotFound(format!("Document '{}' not found", input.document_id))
                })?;

            // Read content: prefer extracted text chunks (works for PDFs, DOCX, etc.)
            // Fall back to raw file read only for plain text files
            let content = {
                let chunks = doc.chunks();
                debug!(
                    document_id = %input.document_id,
                    chunk_count = chunks.len(),
                    "Get document: using aggregate chunks"
                );

                if !chunks.is_empty() {
                    // Reassemble document text from indexed chunks (sorted by position)
                    let mut sorted_chunks: Vec<_> = chunks.iter().collect();
                    sorted_chunks.sort_by_key(|c| c.index());
                    sorted_chunks
                        .iter()
                        .map(|c| c.content())
                        .collect::<Vec<_>>()
                        .join("\n\n")
                } else {
                    // Fallback: try repository (legacy) then raw file
                    let repo_chunks = self
                        .chunk_repository
                        .find_by_document(doc.id().as_str())
                        .await
                        .unwrap_or_default();

                    if !repo_chunks.is_empty() {
                        debug!(
                            document_id = %input.document_id,
                            chunk_count = repo_chunks.len(),
                            "Get document: using repository chunks"
                        );
                        let mut sorted_chunks = repo_chunks;
                        sorted_chunks.sort_by_key(|c| c.index());
                        sorted_chunks
                            .iter()
                            .map(|c| c.content())
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    } else {
                        debug!(
                            document_id = %input.document_id,
                            "Get document: using raw file fallback"
                        );
                        // No chunks — try reading the raw file (works for plain text)
                        self.file_storage
                            .read_file(doc.file_path())
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!(
                                    document_id = %input.document_id,
                                    error = %e,
                                    "Failed to read file content, returning empty"
                                );
                                String::new()
                            })
                    }
                }
            };

            // Paginate content for large documents using max_content_length as page size.
            let page_size_chars = input.max_content_length.max(1);
            let total_chars = content.chars().count();
            let total_pages = std::cmp::max(1, total_chars.div_ceil(page_size_chars));
            let current_page = input.page.clamp(1, total_pages);
            let start_char = (current_page - 1) * page_size_chars;
            let end_char = std::cmp::min(start_char + page_size_chars, total_chars);
            let final_content = Self::slice_by_char_range(&content, start_char, end_char);
            let truncated = total_pages > 1;

            let has_previous_page = current_page > 1;
            let has_next_page = current_page < total_pages;
            let previous_page = has_previous_page.then_some(current_page - 1);
            let next_page = has_next_page.then_some(current_page + 1);
            debug!(
                document_id = %input.document_id,
                content_len = final_content.len(),
                total_chars = total_chars,
                page = current_page,
                total_pages = total_pages,
                truncated = truncated,
                "Get document: content assembled"
            );

            let metadata = if input.include_metadata {
                debug!(
                    document_id = %input.document_id,
                    "Get document: building metadata"
                );
                let size_bytes = match self.file_storage.metadata(doc.file_path()).await {
                    Ok(meta) => meta.size as i64,
                    Err(e) => {
                        tracing::warn!(
                            document_id = %input.document_id,
                            error = %e,
                            "Failed to read file metadata, using stored size"
                        );
                        doc.size_bytes()
                    }
                };

                let tags = match self
                    .tag_service
                    .get_tags_for_document(doc.id().as_str())
                    .await
                {
                    Ok(tags) => tags
                        .into_iter()
                        .map(|tag| tag.name().as_str().to_string())
                        .collect(),
                    Err(e) => {
                        tracing::warn!(
                            document_id = %input.document_id,
                            error = %e,
                            "Failed to fetch tags for document"
                        );
                        Vec::new()
                    }
                };
                debug!(
                    document_id = %input.document_id,
                    tag_count = tags.len(),
                    "Get document: tags loaded"
                );

                let chunk_count = match self
                    .chunk_repository
                    .count_by_document(doc.id().as_str())
                    .await
                {
                    Ok(count) => count.max(0) as usize,
                    Err(e) => {
                        tracing::warn!(
                            document_id = %input.document_id,
                            error = %e,
                            "Failed to fetch chunk count for document"
                        );
                        0
                    }
                };
                debug!(
                    document_id = %input.document_id,
                    chunk_count = chunk_count,
                    "Get document: chunk count loaded"
                );

                Some(DocumentMetadata {
                    filename: doc
                        .file_path()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    file_path: doc.file_path().display().to_string(),
                    mime_type: doc.mime_type().to_string(),
                    extension: doc
                        .file_path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_string(),
                    size_bytes,
                    created_at: None,
                    modified_at: *doc.updated_at(),
                    indexed_at: *doc.indexed_at(),
                    tags,
                    chunk_count,
                })
            } else {
                None
            };

            let output = GetDocumentOutput {
                document_id: input.document_id,
                content: final_content,
                content_truncated: truncated,
                page: current_page,
                total_pages,
                total_chars,
                has_previous_page,
                has_next_page,
                previous_page,
                next_page,
                metadata,
            };

            info!("Get document completed: id='{}'", output.document_id);

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin(async move)
    }

    /// Execute list_documents function
    ///
    /// Boxed to prevent stack overflow in execute() match dispatch.
    pub(super) fn handle_list_documents<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: ListDocumentsInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!(
                "List documents: filter={:?}, limit={}",
                input.filter_mode, input.limit
            );

            let mut documents = self.document_repository.list_metadata().await?;

            let favorite_ids: HashSet<String> = self
                .favorites_repository
                .list_favorites()
                .await?
                .into_iter()
                .map(|favorite| favorite.document_id)
                .collect();

            let recent_limit = input.limit.saturating_add(input.offset);
            let recent_ids: HashSet<String> = if matches!(input.filter_mode, FilterMode::Recent) {
                self.recent_documents_repository
                    .get_recent_documents(recent_limit)
                    .await?
                    .into_iter()
                    .map(|recent| recent.document_id)
                    .collect()
            } else {
                HashSet::new()
            };

            let tag_ids: HashSet<String> = if matches!(input.filter_mode, FilterMode::ByTag) {
                match &input.tags {
                    Some(tags) => {
                        let mut ids = HashSet::new();
                        for tag in tags {
                            for document_id in self.tag_service.search_documents_by_tag(tag).await?
                            {
                                ids.insert(document_id);
                            }
                        }
                        ids
                    }
                    None => HashSet::new(),
                }
            } else {
                HashSet::new()
            };

            let file_types = input.file_types.as_ref().map(|types| {
                types
                    .iter()
                    .map(|t| t.to_lowercase())
                    .collect::<HashSet<_>>()
            });

            documents.retain(|doc| {
                let document_id = doc.id().as_str();

                match input.filter_mode {
                    FilterMode::Recent if !recent_ids.contains(document_id) => return false,
                    FilterMode::Favorites if !favorite_ids.contains(document_id) => return false,
                    FilterMode::ByTag if !tag_ids.contains(document_id) => return false,
                    _ => {}
                }

                if let Some(types) = &file_types {
                    let extension = doc
                        .file_path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if !types.contains(&extension) {
                        return false;
                    }
                }

                if let Some(date_from) = &input.date_from {
                    if doc.modified_at() < date_from {
                        return false;
                    }
                }

                if let Some(date_to) = &input.date_to {
                    if doc.modified_at() > date_to {
                        return false;
                    }
                }

                true
            });

            documents.sort_by(|left, right| {
                let ordering = match input.sort_by {
                    SortField::Modified => left.modified_at().cmp(right.modified_at()),
                    SortField::Created => left.indexed_at().cmp(right.indexed_at()),
                    SortField::Name => left.file_name().cmp(right.file_name()),
                    SortField::Size => left.size_bytes().cmp(&right.size_bytes()),
                };

                match input.sort_order {
                    SortOrder::Asc => ordering,
                    SortOrder::Desc => ordering.reverse(),
                }
            });

            let total = documents.len();
            let start = input.offset.min(total);
            let end = (start + input.limit).min(total);
            let page = documents.into_iter().skip(start).take(end - start);

            let mut items = Vec::new();
            for doc in page {
                let document_id = doc.id().as_str().to_string();
                let tags = self
                    .tag_service
                    .get_tags_for_document(doc.id().as_str())
                    .await?
                    .into_iter()
                    .map(|tag| tag.name().as_str().to_string())
                    .collect();

                let extension = doc
                    .file_path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_string();

                items.push(DocumentListItem {
                    document_id: document_id.clone(),
                    filename: doc.file_name().to_string(),
                    file_path: doc.file_path().display().to_string(),
                    mime_type: doc.mime_type().to_string(),
                    extension,
                    size_bytes: doc.size_bytes(),
                    modified_at: *doc.modified_at(),
                    tags,
                    is_favorite: favorite_ids.contains(&document_id),
                    access_count: doc.access_count().max(0) as usize,
                });
            }

            let output = ListDocumentsOutput {
                documents: items,
                total,
                limit: input.limit,
                offset: input.offset,
                has_more: end < total,
            };

            info!("List documents completed: {} documents", output.total);

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin handle_list_documents
    }
}
