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
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Tag generation request DTO with document ID and optional parameters
///
/// # Returns
///
/// * `Ok(GenerateTagsResponseDto)` - Generated tags with confidence scores
/// * `Err(AppError)` - If the LLM is unavailable or generation fails
///
/// # Errors
///
/// * `AppError::NotFound` - Document ID does not exist
/// * `AppError::Other` - LLM unavailable, generation failed, or document not indexed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface GenerateTagsRequest {
///   documentId: string;
///   maxTags?: number;
///   existingTags?: string[];
/// }
///
/// interface GenerateTagsResponse {
///   tags: Array<{
///     name: string;
///     confidence: number;
///     rationale?: string;
///   }>;
///   modelUsed: string;
/// }
///
/// // Generate tags for a document
/// const response = await invoke<GenerateTagsResponse>('generate_tags_ddd', {
///   request: {
///     documentId: 'doc_123',
///     maxTags: 5
///   }
/// });
///
/// console.log(`Generated ${response.tags.length} tags using ${response.modelUsed}`);
/// response.tags.forEach(tag => {
///   console.log(`- ${tag.name} (confidence: ${tag.confidence})`);
/// });
///
/// // Apply generated tags with user confirmation
/// const confirmedTags = response.tags
///   .filter(tag => tag.confidence > 0.7)
///   .map(tag => tag.name);
///
/// await invoke('apply_tags_ddd', {
///   request: {
///     documentId: 'doc_123',
///     tagNames: confirmedTags
///   }
/// });
/// ```
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
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Delegates to `GenerateTagsUseCase` (LLM prompting, tag extraction)
///
/// # Command Flow
///
/// 1. Execute GenerateTagsUseCase:
///    - Fetch document content
///    - Build LLM prompt
///    - Call LLM API
///    - Parse and validate tags
///    - Score confidence
/// 2. Return generated tags with metadata
///
/// # Performance
///
/// - **Typical Response Time**: 2-10 seconds (depends on LLM and document length)
/// - **Cost**: LLM API calls incur costs (varies by model and provider)
/// - **Optimization**: Cache document embeddings, reuse for multiple operations
///
/// # Use Cases
///
/// - **Automated Tagging**: Suggest tags to users during document import
/// - **Bulk Organization**: Tag large document collections automatically
/// - **Tag Suggestion**: Provide tag recommendations in UI
/// - **Content Analysis**: Understand document themes and topics
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn generate_tags_impl(
    container: &Container,
    request: GenerateTagsRequestDto,
) -> Result<GenerateTagsResponseDto> {
    container.default_chat_llm().await?;

    let use_case = container.generate_tags_use_case();
    use_case.execute(request).await
}

/// ## Tauri Command Layer (Thin Wrapper)
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
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Auto-tag request DTO with max documents and generation parameters
///
/// # Returns
///
/// * `Ok(AutoTagResponseDto)` - Statistics (tagged count, skipped, errors)
/// * `Err(AppError)` - If validation or the operation fails
///
/// # Errors
///
/// * `AppError::InvalidInput` - Batch size exceeds hard limit (max 100 documents)
/// * `AppError::Other` - LLM unavailable or bulk operation failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface AutoTagRequest {
///   maxDocuments: number;
///   tagsPerDocument?: number;
///   onlyUntagged?: boolean;
/// }
///
/// interface AutoTagResponse {
///   totalProcessed: number;
///   successCount: number;
///   skippedCount: number;
///   errorCount: number;
///   errors?: Array<{ documentId: string; error: string }>;
/// }
///
/// // Auto-tag up to 50 untagged documents
/// const response = await invoke<AutoTagResponse>('auto_tag_all_documents_ddd', {
///   request: {
///     maxDocuments: 50,
///     tagsPerDocument: 5,
///     onlyUntagged: true
///   }
/// });
///
/// console.log(`Tagged ${response.successCount}/${response.totalProcessed} documents`);
/// console.log(`Skipped: ${response.skippedCount}, Errors: ${response.errorCount}`);
///
/// if (response.errors && response.errors.length > 0) {
///   console.error('Failed documents:', response.errors);
/// }
/// ```
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
/// # Performance
///
/// - **Typical Time**: 2-10 seconds per document (LLM generation time)
/// - **100 Documents**: ~5-15 minutes (with throttling)
/// - **Cost**: LLM API costs scale with document count
/// - **Optimization**: Batch processing amortizes overhead
///
/// # Response Statistics
///
/// - **totalProcessed**: Number of documents attempted
/// - **successCount**: Successfully tagged documents
/// - **skippedCount**: Documents skipped (already tagged, no content, etc.)
/// - **errorCount**: Documents that failed to tag
/// - **errors**: Array of {documentId, error} for failed documents
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// 1. Validates batch size (hard limit enforcement)
/// 2. Delegates to `AutoTagDocumentsUseCase` (batch processing, throttling, error handling)
///
/// # Command Flow
///
/// 1. Validate batch size (<= 100 documents)
/// 2. Execute AutoTagDocumentsUseCase:
///    - Find candidate documents (untagged or all)
///    - For each document in batch:
///      - Generate tags via LLM
///      - Apply tags to document
///      - Handle errors gracefully
///      - Throttle between documents
/// 3. Return statistics
///
/// # Use Cases
///
/// - **Initial Organization**: Tag all documents after first import
/// - **Re-Tagging**: Update tags after model improvements
/// - **Maintenance**: Periodically tag new untagged documents
/// - **Migration**: Bulk tag documents moved from other systems
///
/// # Best Practices
///
/// - **Small Batches**: Start with 10-20 documents to test
/// - **Monitor Costs**: Track LLM API usage and costs
/// - **User Feedback**: Allow users to review generated tags
/// - **Respect Limits**: Don't exceed 100 document hard limit
/// - **Batch Delays**: Wait 1+ minute between batches for large collections
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
