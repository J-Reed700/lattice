//! LLM tag suggestions for one document and auto-tagging the library.

use tauri::State;

use crate::features::tags::dto::*;
use crate::interfaces::di::Container;
use lattice_core::shared::error::{AppError, Result};

/// Generates tags for a document using LLM analysis
///
/// Automatically generates relevant tags for a document by analyzing its content
/// with a Large Language Model. The LLM reads the document text and suggests
/// appropriate categorization tags based on the content's topics, themes, and
/// subject matter. This is useful for bulk tagging, automated workflows, and
/// suggesting tags to users.
///
/// # Errors
///
/// * `AppError::NotFound` - Document ID does not exist
/// * `AppError::Other` - LLM unavailable, generation failed, or document not indexed
///
/// # LLM Tag Generation Process
///
/// 1. **Content Retrieval**: Fetch document content from index
/// 2. **Context Building**: Prepare prompt with document text and instructions
/// 3. **LLM Analysis**: Send to configured LLM for tag suggestions
/// 4. **Tag Extraction**: Parse LLM response and extract tag names
/// 5. **Confidence Scoring**: Evaluate tag relevance (if LLM provides scores)
/// 6. **Deduplication**: Remove duplicate or overlapping tags
///
/// # Generation Parameters
///
/// - **maxTags**: Limit number of tags generated (default: 5, recommended: 3-10)
/// - **existingTags**: Provide context of already-applied tags to avoid duplicates
/// - **Model**: Uses the configured LLM model
///
/// # Tag Quality
///
/// Generated tags are:
/// - **Relevant**: Based on actual document content, not guesses
/// - **Specific**: More specific than generic categories when appropriate
/// - **Actionable**: Useful for search and organization
/// - **Confidence-Scored**: Higher confidence = more relevant
pub async fn generate_tags_impl(
    container: &Container,
    request: GenerateTagsRequestDto,
) -> Result<GenerateTagsResponseDto> {
    container.default_chat_llm().await?;

    let use_case = container.generate_tags_use_case();
    use_case.execute(request).await
}

pub async fn generate_tags_ddd(
    container: State<'_, Container>,
    request: GenerateTagsRequestDto,
) -> Result<GenerateTagsResponseDto> {
    generate_tags_impl(container.inner(), request).await
}

/// Automatically tags multiple untagged documents using LLM analysis
///
/// Bulk operation that generates and applies tags to multiple documents using LLM
/// analysis. Processes documents in batches with throttling to prevent resource
/// exhaustion. Useful for tagging large document collections after initial import
/// or for periodic re-tagging operations.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Batch size exceeds hard limit (max 100 documents)
/// * `AppError::Other` - LLM unavailable or bulk operation failed
///
/// # Security
///
/// - **Hard Batch Limit (CWE-770)**: Maximum 100 documents per batch (DoS prevention)
/// - **Input Validation**: Batch size validated before processing
///
/// # Hard Limits (DoS Prevention)
///
/// **CRITICAL SECURITY**: Batch size is hard-limited to prevent abuse:
/// - **Max Batch Size**: 100 documents per request
/// - **Rationale**: LLM calls are expensive (time, compute, cost)
/// - **Enforcement**: Request rejected if `maxDocuments > 100`
/// - **Workaround**: Process large collections in multiple batches
///
/// Example of proper batching:
/// ```typescript
/// async function tagAllDocuments(documentIds: string[]) {
///   const BATCH_SIZE = 100;
///   for (let i = 0; i < documentIds.length; i += BATCH_SIZE) {
///     const batch = documentIds.slice(i, i + BATCH_SIZE);
///     await invoke('auto_tag_all_documents_ddd', {
///       request: { maxDocuments: batch.length }
///     });
///   }
/// }
/// ```
///
/// # Processing Behavior
///
/// - **Batch Processing**: Documents processed in configurable batches
/// - **Error Handling**: Individual document failures don't stop batch processing
/// - **Progress**: Returns statistics for monitoring
/// - **Filtering**: Can filter to only untagged documents (`onlyUntagged: true`)
///
/// # Response Statistics
///
/// - **totalProcessed**: Number of documents attempted
/// - **successCount**: Successfully tagged documents
/// - **skippedCount**: Documents skipped (already tagged, no content, etc.)
/// - **errorCount**: Documents that failed to tag
/// - **errors**: Array of {documentId, error} for failed documents
pub async fn auto_tag_all_documents_ddd(
    container: State<'_, Container>,
    request: AutoTagRequestDto,
) -> Result<AutoTagResponseDto> {
    // SECURITY: Enforce hard limit
    const MAX_AUTO_TAG_DOCUMENTS: usize = 100;
    if request.max_documents > MAX_AUTO_TAG_DOCUMENTS {
        return Err(AppError::InvalidInput(format!(
            "Batch size {} exceeds maximum {}. Please tag documents in smaller batches.",
            request.max_documents, MAX_AUTO_TAG_DOCUMENTS
        )));
    }

    container.default_chat_llm().await?;

    let use_case = container.auto_tag_all_documents_use_case();
    let response = use_case.execute(request).await?;

    Ok(response)
}
