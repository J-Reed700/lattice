//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::features::search::engine::service::SearchResult;
use crate::infrastructure::services::context_manager::LLMContext;
use crate::shared::error::Result;
use async_trait::async_trait;

#[async_trait]
pub trait QAEngineTrait: Send + Sync {
    /// Answer a question using the RAG pipeline (non-streaming)
    ///
    /// # Arguments
    /// * `question` - User's question
    /// * `search_results` - Relevant documents from search
    /// * `max_context_tokens` - Maximum tokens for context (default: 2000)
    /// * `llm_context` - Optional conversation context for conversational Q&A
    ///   - `None` = Non-conversational mode (uses default system prompt)
    ///   - `Some(context)` = Conversational mode (uses history + custom system prompt)
    ///
    /// # Returns
    /// Generated answer as a complete string
    ///
    /// # Errors
    /// - `QAError::InvalidInput` if question is empty or max_context_tokens is 0
    /// - `QAError::OllamaUnavailable` if LLM is not reachable
    /// - `QAError::InternalError` if generation fails
    ///
    /// # Example
    /// ```rust
    /// // Non-conversational
    /// let answer = engine.answer("What is Rust?", results, 2000, None).await?;
    ///
    /// // Conversational with context
    /// let answer = engine.answer("What is Rust?", results, 2000, Some(llm_context)).await?;
    /// ```
    async fn answer(
        &self,
        question: &str,
        search_results: Vec<SearchResult>,
        max_context_tokens: usize,
        llm_context: Option<LLMContext>,
    ) -> Result<String, crate::features::qa::engine::types::QAError>;

    /// Answer a question with streaming response
    ///
    /// Returns a stream of chunks (tokens, sources, done, or error).
    ///
    /// # Arguments
    /// * `question` - User's question
    /// * `search_results` - Relevant documents from search
    /// * `max_context_tokens` - Maximum tokens for context (default: 2000)
    /// * `llm_context` - Optional conversation context for conversational Q&A
    ///   - `None` = Non-conversational mode (uses default system prompt)
    ///   - `Some(context)` = Conversational mode (uses history + custom system prompt)
    ///
    /// # Returns
    /// Stream of StreamChunk items (Token, Sources, Done, Error)
    ///
    /// # Errors
    /// - `QAError::InvalidInput` if question is empty or max_context_tokens is 0
    /// - `QAError::OllamaUnavailable` if LLM is not reachable
    ///
    /// # Example
    /// ```rust
    /// // Non-conversational
    /// let mut stream = engine.answer_stream("What is Rust?", results, 2000, None).await?;
    /// while let Some(chunk) = stream.next().await {
    ///     match chunk {
    ///         StreamChunk::Token { content } => print!("{}", content),
    ///         StreamChunk::Done => break,
    ///         _ => {}
    ///     }
    /// }
    /// ```
    async fn answer_stream(
        &self,
        question: &str,
        search_results: Vec<SearchResult>,
        max_context_tokens: usize,
        llm_context: Option<LLMContext>,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn tokio_stream::Stream<Item = crate::features::qa::engine::types::StreamChunk>
                    + Send
                    + '_,
            >,
        >,
        crate::features::qa::engine::types::QAError,
    >;

    /// Check if LLM client is available
    ///
    /// # Returns
    /// True if the LLM client is reachable and the model is available
    async fn health_check(&self) -> bool;

    /// Get the configured model name
    ///
    /// # Returns
    /// Model name string (e.g., "llama3.1:8b", "claude-sonnet-4")
    fn model_name(&self) -> &str;
}

/// Trait for conversational question-answering operations
///
/// Orchestrates the complete conversational Q&A pipeline combining:
/// - Conversation context (history, system prompt)
/// - Document retrieval (RAG)
/// - LLM generation (streaming and non-streaming)
/// - Message persistence
///
/// # Implementations
/// - `ConversationalQAService`: Production implementation with full RAG pipeline
/// - `MockConversationalQAService`: Mock for testing
#[async_trait]
#[async_trait]
pub trait ConversationalQAServiceTrait: Send + Sync {
    /// Ask a question within a conversation context (non-streaming)
    ///
    /// Orchestrates: load conversation → search docs → build context → generate answer → save messages
    ///
    /// # Arguments
    /// * `conversation_id` - ID of the conversation
    /// * `question` - User's question
    /// * `search_results` - Relevant documents from search
    ///
    /// # Returns
    /// ConversationalAnswer with generated answer, sources, and conversation stats
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Other` if Q&A generation fails
    /// - `AppError::Database` if saving messages fails
    ///
    /// # Example
    /// ```rust
    /// let answer = service.ask_question(
    ///     "conv-123",
    ///     "What is machine learning?",
    ///     search_results,
    /// ).await?;
    /// ```
    async fn ask_question(
        &self,
        conversation_id: &str,
        question: &str,
        search_results: Vec<crate::features::search::dto::SearchResultDto>,
    ) -> Result<crate::features::qa::conversational_service::ConversationalAnswer>;

    /// Ask a question with streaming response
    ///
    /// Similar to `ask_question`, but streams answer token-by-token for better UX.
    ///
    /// # Arguments
    /// * `conversation_id` - ID of the conversation
    /// * `question` - User's question
    /// * `search_results` - Relevant documents from search
    /// * `window` - Tauri window for emitting stream events
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Other` if streaming fails
    /// - `AppError::Database` if saving messages fails
    ///
    /// # Side Effects
    /// - Emits "llm-stream" events to frontend
    /// - Saves messages to database after streaming completes
    async fn ask_question_stream(
        &self,
        conversation_id: &str,
        question: &str,
        search_results: Vec<crate::features::search::dto::SearchResultDto>,
        window: tauri::Window,
    ) -> Result<()>;
}
