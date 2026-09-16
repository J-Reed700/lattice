//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::features::search::engine::service::SearchResult;
use crate::shared::error::Result;
use async_trait::async_trait;

/// Trait for managing LLM context in conversational RAG
///
/// Handles formatting and token budgeting across three context types:
/// - System prompts
/// - Conversation history
/// - Retrieved documents
///
/// # Implementations
/// - `ContextManager`: Production implementation with token budgeting
/// - `MockContextManager`: Mock for testing
pub trait ContextManagerTrait: Send + Sync {
    /// Build complete context for LLM API call
    ///
    /// Combines all three context types (system prompt, conversation history,
    /// document context) into a single LLMContext ready for the API.
    ///
    /// # Arguments
    /// * `conversation` - Conversation aggregate with history and system prompt
    /// * `search_results` - RAG search results to include as context
    ///
    /// # Returns
    /// LLMContext with all sections formatted and token-counted
    ///
    /// # Errors
    /// Returns error if token budget is insufficient
    fn build_context_for_llm(
        &self,
        conversation: &crate::domain::conversation::ConversationAggregate,
        search_results: Vec<SearchResult>,
    ) -> Result<crate::infrastructure::services::context_manager::LLMContext>;

    /// Format system context (system prompt)
    ///
    /// # Arguments
    /// * `system_prompt` - Optional custom system prompt
    ///
    /// # Returns
    /// Formatted system prompt string
    fn format_system_context(&self, system_prompt: Option<&str>) -> String;

    /// Format document context from search results
    ///
    /// # Arguments
    /// * `search_results` - Search results to format
    /// * `max_tokens` - Maximum tokens for document context
    ///
    /// # Returns
    /// Formatted document context string
    ///
    /// # Errors
    /// Returns error if truncation fails
    fn format_document_context(
        &self,
        search_results: Vec<SearchResult>,
        max_tokens: usize,
    ) -> Result<String>;

    /// Format conversation history as LLM messages
    ///
    /// # Arguments
    /// * `messages` - Conversation messages to format
    ///
    /// # Returns
    /// Vector of LLMMessage objects
    fn format_conversation_history(
        &self,
        messages: &[crate::domain::conversation::ConversationMessage],
    ) -> Vec<crate::domain::conversation::LLMMessage>;

    /// Get maximum context token budget
    fn max_context_tokens(&self) -> usize;
}

/// Trait for question-answering engine with RAG pipeline
///
/// Combines semantic search results with LLM generation to answer questions.
/// This is a lower-level primitive used by ConversationalQAService.
///
/// # Implementations
/// - `QAEngine`: Production implementation with LLM clients (Ollama, Anthropic, Local)
/// - `MockQAEngine`: Mock for testing without actual LLM calls
#[async_trait]
#[async_trait]
pub trait SearchEnrichmentServiceTrait: Send + Sync {
    /// Enrich search results with document metadata
    ///
    /// Given chunk IDs from search results, fetches associated metadata including
    /// document titles, file paths, creation dates, and content snippets.
    ///
    /// # Performance
    /// - Processes chunks in batches of 900 (SQLite parameter limit is 999)
    /// - Uses parallel batch processing with concurrency=4
    /// - Expected speedup: 3-4x over sequential processing
    ///
    /// # Arguments
    /// * `chunk_ids` - Slice of chunk IDs to enrich (can be any size)
    ///
    /// # Returns
    /// HashMap mapping chunk ID to DocumentMetadata containing:
    /// - snippet: Content preview (max 200 chars)
    /// - metadata: Hash map with filename, file_type, file_size, created_at, updated_at
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    ///
    /// # Example
    /// ```rust
    /// let chunk_ids: Vec<String> = search_results.iter().map(|r| r.id.clone()).collect();
    /// let enriched = service.enrich_results(&chunk_ids).await?;
    ///
    /// for result in search_results {
    ///     if let Some(metadata) = enriched.get(&result.id) {
    ///         println!("Snippet: {}", metadata.snippet);
    ///     }
    /// }
    /// ```
    async fn enrich_results(
        &self,
        chunk_ids: &[String],
    ) -> Result<
        std::collections::HashMap<
            String,
            crate::features::search::enrichment_service::DocumentMetadata,
        >,
    >;
}
