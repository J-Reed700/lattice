//! Conversational Q&A Service
//!
//! Business logic layer for conversational question-answering with RAG.
//! This service orchestrates the complete conversational Q&A pipeline:
//! - Load conversation context
//! - Search relevant documents
//! - Build LLM context with conversation history + documents
//! - Generate answer (streaming or non-streaming)
//! - Save messages to conversation
//! - Record metrics
//!
//! # Architecture
//!
//! This service follows the "bricks and studs" philosophy:
//! - **Self-contained**: All conversational Q&A business logic in one place
//! - **Clear boundaries**: Depends on well-defined service traits
//! - **Testable**: Easy to inject mocks for all dependencies
//! - **Regeneratable**: Can be rebuilt from this specification
//!
//! # Design Principles
//!
//! - **Single Responsibility**: Handles only conversational Q&A orchestration
//! - **Dependency Inversion**: Depends on abstractions (traits), not concrete types
//! - **Fat Service, Thin Command**: Business logic here, commands delegate
//!
//! # Usage
//!
//! ```rust
//! // In command handler
//! let service = container.conversational_qa_service();
//! let answer = service.ask_question(
//!     "conv-123",
//!     "What is machine learning?",
//!     search_results,
//! ).await?;
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::features::search::dto::SearchResultDto;
use crate::domain::ValidatedMetadata;
use crate::infrastructure::observability::Metrics;
use crate::infrastructure::qa::types::StreamChunk;
use crate::infrastructure::services::traits::{
    ContextManagerTrait, ConversationServiceTrait, ConversationalQAServiceTrait, QAEngineTrait,
};
use crate::shared::error::{AppError, Result};

// ============================================================================
// Response Types
// ============================================================================

/// Response from conversational Q&A (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationalAnswer {
    /// Generated answer text
    pub answer: String,
    /// Source documents used for RAG
    pub sources: Vec<SearchResultDto>,
    /// Conversation ID
    pub conversation_id: String,
    /// Total message count in conversation
    pub message_count: i64,
    /// Total tokens used in conversation
    pub total_tokens: i64,
}

// ============================================================================
// Service Implementation
// ============================================================================

/// Service for conversational question-answering with RAG
///
/// Orchestrates the complete pipeline for asking questions within a conversation context:
/// 1. Load conversation (history, system prompt, context)
/// 2. Build LLM context from conversation history + search results
/// 3. Generate answer using Q&A engine
/// 4. Save user question and assistant answer as messages
/// 5. Record metrics for observability
///
/// # Dependencies
///
/// - `ConversationService`: Manages conversation persistence and history
/// - `ContextManager`: Builds LLM context with token budgeting
/// - `QAEngine`: Generates answers using LLM
/// - `Metrics`: Records request duration and errors
///
/// # Example
///
/// ```rust
/// let service = ConversationalQAService::new(
///     conversation_service,
///     context_manager,
///     qa_engine,
///     metrics,
/// );
///
/// let answer = service.ask_question(
///     "conv-abc123",
///     "What is gradient descent?",
///     search_results,
/// ).await?;
///
/// println!("Answer: {}", answer.answer);
/// println!("Sources: {}", answer.sources.len());
/// println!("Total messages: {}", answer.message_count);
/// ```
pub struct ConversationalQAService {
    /// Conversation persistence and management
    conversation_service: Arc<dyn ConversationServiceTrait>,

    /// LLM context building with token budgeting
    context_manager: Arc<dyn ContextManagerTrait>,

    /// Q&A engine for answer generation
    qa_engine: Arc<dyn QAEngineTrait>,

    /// Metrics for observability
    metrics: Arc<Metrics>,
}

impl ConversationalQAService {
    /// Create a new conversational Q&A service
    ///
    /// # Arguments
    ///
    /// * `conversation_service` - Service for conversation persistence
    /// * `context_manager` - Service for LLM context building
    /// * `qa_engine` - Q&A engine for answer generation
    /// * `metrics` - Metrics collector for observability
    ///
    /// # Returns
    ///
    /// New ConversationalQAService instance
    pub fn new(
        conversation_service: Arc<dyn ConversationServiceTrait>,
        context_manager: Arc<dyn ContextManagerTrait>,
        qa_engine: Arc<dyn QAEngineTrait>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            conversation_service,
            context_manager,
            qa_engine,
            metrics,
        }
    }

    /// Ask a question within a conversation context (non-streaming)
    ///
    /// Orchestrates the complete conversational Q&A pipeline:
    /// 1. Load conversation aggregate (history + metadata)
    /// 2. Build LLM context from conversation history + search results
    /// 3. Generate answer using Q&A engine
    /// 4. Save user question and assistant answer as messages
    /// 5. Record metrics (duration, tokens)
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - ID of the conversation
    /// * `question` - User's question
    /// * `search_results` - Relevant documents from search
    ///
    /// # Returns
    ///
    /// ConversationalAnswer with:
    /// - Generated answer text
    /// - Source documents
    /// - Updated conversation stats (message count, total tokens)
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Other` if Q&A generation fails
    /// - `AppError::Database` if saving messages fails
    ///
    /// # Example
    ///
    /// ```rust
    /// let answer = service.ask_question(
    ///     "conv-123",
    ///     "What is backpropagation?",
    ///     search_results,
    /// ).await?;
    ///
    /// println!("Answer: {}", answer.answer);
    /// ```
    pub async fn ask_question(
        &self,
        conversation_id: &str,
        question: &str,
        search_results: Vec<SearchResultDto>,
    ) -> Result<ConversationalAnswer> {
        let start_time = std::time::Instant::now();

        info!(
            conversation_id = %conversation_id,
            question = %question,
            num_results = search_results.len(),
            "Processing conversational Q&A"
        );

        // 1. Load conversation aggregate
        let aggregate = self
            .conversation_service
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Conversation not found: {}", conversation_id))
            })?;

        // 2. Convert search results to infrastructure format
        let infra_search_results = self.convert_search_results(&search_results);

        // 3. Build LLM context (conversation history + document context)
        let llm_context = self
            .context_manager
            .build_context_for_llm(&aggregate, infra_search_results.clone())?;

        info!(
            num_messages = aggregate.message_count(),
            total_tokens = aggregate.total_tokens(),
            "Built LLM context from conversation history"
        );

        // 4. Use same converted results for Q&A engine
        let qa_search_results = infra_search_results;

        // 4. Generate answer using Q&A engine with conversation context
        let answer = self
            .qa_engine
            .answer(question, qa_search_results, 2000, Some(llm_context))
            .await
            .map_err(|e| {
                self.metrics.record_llm_error();
                AppError::Other(format!("Q&A generation failed: {}", e))
            })?;

        // 5. Estimate tokens (simple heuristic: ~4 chars per token)
        let question_tokens = (question.len() / 4) as i64;
        let answer_tokens = (answer.len() / 4) as i64;

        // 6. Serialize and validate sources metadata
        let sources_json = serde_json::to_string(&search_results).map_err(|e| {
            warn!("Failed to serialize sources: {}", e);
            AppError::Other(format!("Failed to serialize sources: {}", e))
        })?;

        let validated_metadata = ValidatedMetadata::new(sources_json).map_err(|e| {
            warn!(
                "Metadata validation failed for {} sources: {}",
                search_results.len(),
                e
            );
            AppError::Other(format!(
                "Cannot save answer: Too many sources ({} sources). \
                     Try asking a more specific question to get fewer sources.",
                search_results.len()
            ))
        })?;

        info!(
            "Metadata validated: {} bytes ({} sources)",
            validated_metadata.size_bytes(),
            search_results.len()
        );

        // 7. Save messages via service
        self.conversation_service
            .add_user_message(conversation_id, question.to_string(), question_tokens)
            .await?;

        self.conversation_service
            .add_assistant_message_with_metadata(
                conversation_id,
                answer.clone(),
                answer_tokens,
                Some(validated_metadata.as_str().to_string()),
            )
            .await?;

        // 8. Get updated conversation stats
        let updated_aggregate = self
            .conversation_service
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Conversation not found: {}", conversation_id))
            })?;

        // 8. Record metrics
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.metrics.record_llm_request(duration_ms);

        info!(
            duration_ms = duration_ms,
            answer_length = answer.len(),
            message_count = updated_aggregate.message_count(),
            total_tokens = updated_aggregate.total_tokens(),
            "Conversational Q&A completed"
        );

        Ok(ConversationalAnswer {
            answer,
            sources: search_results,
            conversation_id: conversation_id.to_string(),
            message_count: updated_aggregate.message_count(),
            total_tokens: updated_aggregate.total_tokens(),
        })
    }

    /// Ask a question with streaming response
    ///
    /// Similar to `ask_question`, but streams the answer token-by-token for better UX.
    /// The complete answer is accumulated and saved after streaming completes.
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - ID of the conversation
    /// * `question` - User's question
    /// * `search_results` - Relevant documents from search
    /// * `window` - Tauri window for emitting stream events
    ///
    /// # Returns
    ///
    /// Unit result after streaming completes successfully
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Other` if streaming fails
    /// - `AppError::Database` if saving messages fails
    ///
    /// # Side Effects
    ///
    /// - Emits "llm-stream" events to frontend with chunks
    /// - Saves messages to database after streaming completes
    ///
    /// # Example
    ///
    /// ```rust
    /// service.ask_question_stream(
    ///     "conv-123",
    ///     "Explain neural networks",
    ///     search_results,
    ///     window,
    /// ).await?;
    /// ```
    pub async fn ask_question_stream(
        &self,
        conversation_id: &str,
        question: &str,
        search_results: Vec<SearchResultDto>,
        window: tauri::Window,
    ) -> Result<()> {
        info!(
            conversation_id = %conversation_id,
            question = %question,
            num_results = search_results.len(),
            "Processing streaming conversational Q&A"
        );

        // 1. Load conversation aggregate
        let aggregate = self
            .conversation_service
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Conversation not found: {}", conversation_id))
            })?;

        // 2. Convert search results to infrastructure format
        let infra_search_results = self.convert_search_results(&search_results);

        // 3. Build LLM context (conversation history + document context)
        let llm_context = self
            .context_manager
            .build_context_for_llm(&aggregate, infra_search_results.clone())?;

        info!(
            num_messages = aggregate.message_count(),
            total_tokens = aggregate.total_tokens(),
            "Built LLM context for streaming from conversation history"
        );

        // 4. Use same converted results for Q&A engine
        let qa_search_results = infra_search_results;

        // 5. Get streaming response from Q&A engine with conversation context
        let mut stream = self
            .qa_engine
            .answer_stream(question, qa_search_results, 2000, Some(llm_context))
            .await
            .map_err(|e| {
                self.metrics.record_llm_error();
                AppError::Other(format!("Streaming Q&A failed: {}", e))
            })?;

        // 4. Emit chunks to frontend and accumulate answer
        let mut had_error = false;
        let mut full_answer = String::new();

        while let Some(chunk) = stream.next().await {
            // Accumulate content chunks
            if let StreamChunk::Token { content } = &chunk {
                full_answer.push_str(content);
            }

            // Emit to frontend
            window
                .emit_to(window.label(), "llm-stream", &chunk)
                .map_err(|e| AppError::Other(format!("Failed to emit event: {}", e)))?;

            // Check for completion or error
            if matches!(chunk, StreamChunk::Done) {
                break;
            }

            if let StreamChunk::Error { .. } = chunk {
                had_error = true;
                break;
            }
        }

        // 5. Save messages if successful
        if !had_error {
            let question_tokens = (question.len() / 4) as i64;
            let answer_tokens = (full_answer.len() / 4) as i64;

            // Serialize and validate sources metadata
            let sources_json = serde_json::to_string(&search_results).map_err(|e| {
                warn!("Failed to serialize sources: {}", e);
                AppError::Other(format!("Failed to serialize sources: {}", e))
            })?;

            let validated_metadata = ValidatedMetadata::new(sources_json).map_err(|e| {
                warn!(
                    "Metadata validation failed for {} sources: {}",
                    search_results.len(),
                    e
                );
                AppError::Other(format!(
                    "Cannot save answer: Too many sources ({} sources). \
                         Try asking a more specific question to get fewer sources.",
                    search_results.len()
                ))
            })?;

            info!(
                "Metadata validated: {} bytes ({} sources)",
                validated_metadata.size_bytes(),
                search_results.len()
            );

            self.conversation_service
                .add_user_message(conversation_id, question.to_string(), question_tokens)
                .await?;

            self.conversation_service
                .add_assistant_message_with_metadata(
                    conversation_id,
                    full_answer,
                    answer_tokens,
                    Some(validated_metadata.as_str().to_string()),
                )
                .await?;

            info!("Streaming conversational Q&A completed");
        } else {
            warn!("Streaming conversational Q&A encountered error");
        }

        Ok(())
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Convert SearchResult to Q&A engine SearchResult format
    ///
    /// The command layer uses a different SearchResult type than the Q&A engine.
    /// This helper converts between them.
    ///
    /// # Arguments
    ///
    /// * `results` - Search results from command layer
    ///
    /// # Returns
    ///
    /// Vector of search results in Q&A engine format
    fn convert_search_results(
        &self,
        results: &[SearchResultDto],
    ) -> Vec<crate::infrastructure::search::service::SearchResult> {
        results
            .iter()
            .map(|r| crate::infrastructure::search::service::SearchResult {
                id: r.id.clone(),
                score: r.score,
                index: 0,
                filename: r
                    .metadata
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                mime_type: r
                    .metadata
                    .get("file_type")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                size_bytes: r.metadata.get("size_bytes").and_then(|v| v.as_i64()),
                created_at: r
                    .metadata
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                content: Some(r.content.clone()),
                file_id: Some(r.id.clone()),
                file_path: r
                    .metadata
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                file_name: r
                    .metadata
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                file_extension: None,
                file_category: None,
                is_indexed: None,
                document_id: r.document_id.clone(),
                snippet: Some(r.content.clone()),
                chunk_index: r.position,
                updated_at: None,
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl ConversationalQAServiceTrait for ConversationalQAService {
    async fn ask_question(
        &self,
        conversation_id: &str,
        question: &str,
        search_results: Vec<crate::features::search::dto::SearchResultDto>,
    ) -> Result<ConversationalAnswer> {
        ConversationalQAService::ask_question(self, conversation_id, question, search_results).await
    }

    async fn ask_question_stream(
        &self,
        conversation_id: &str,
        question: &str,
        search_results: Vec<crate::features::search::dto::SearchResultDto>,
        window: tauri::Window,
    ) -> Result<()> {
        ConversationalQAService::ask_question_stream(
            self,
            conversation_id,
            question,
            search_results,
            window,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::qa::QAEngine;
    use crate::infrastructure::services::traits::{MockContextManager, MockConversationService};
    use crate::llm::OllamaClient;

    #[tokio::test]
    async fn test_conversational_qa_service_creation() {
        // Create mock dependencies
        let conversation_service =
            Arc::new(MockConversationService::new()) as Arc<dyn ConversationServiceTrait>;
        let context_manager = Arc::new(MockContextManager::new()) as Arc<dyn ContextManagerTrait>;
        let llm_client = Arc::new(OllamaClient::new("http://localhost:11434").unwrap());
        let qa_engine = Arc::new(QAEngine::new(
            llm_client as Arc<dyn crate::llm::traits::LLMClient>,
        )) as Arc<dyn QAEngineTrait>;
        let metrics = Arc::new(Metrics::new());

        // Create service
        let _service =
            ConversationalQAService::new(conversation_service, context_manager, qa_engine, metrics);

        // Just verify it compiles and constructs
    }

    #[test]
    fn test_metadata_size_validation_accepts_normal() {
        // Normal case with ~10 sources (typical search results)
        let normal_results: Vec<SearchResultDto> = (0..10)
            .map(|i| SearchResultDto {
                id: format!("chunk_{}", i),
                title: "test.txt".to_string(),
                content: "Normal content preview for search results".to_string(),
                score: 0.8,
                path: Some("/path/test.txt".to_string()),
                document_id: Some(format!("doc_{}", i)),
                position: Some(i),
                vector_score: Some(0.8),
                bm25_score: None,
                vector_rank: None,
                bm25_rank: None,
                metadata: std::collections::HashMap::new(),
            })
            .collect();

        let json = serde_json::to_string(&normal_results).unwrap();
        println!("Normal JSON size: {} bytes", json.len());

        let result = ValidatedMetadata::new(json);
        assert!(result.is_ok(), "Should accept normal-sized metadata");

        let metadata = result.unwrap();
        assert!(
            metadata.size_bytes() < 10_000,
            "Normal metadata should be < 10KB"
        );
    }

    #[test]
    fn test_metadata_size_validation_rejects_oversized() {
        // Create oversized search results (1000 sources with large content)
        let large_results: Vec<SearchResultDto> = (0..1000)
            .map(|i| SearchResultDto {
                id: format!("chunk_{}", i),
                title: format!("file_{}.txt", i),
                content: "x".repeat(1000), // 1000 bytes each = ~1MB total
                score: 0.5,
                path: Some(format!("/path/to/file_{}.txt", i)),
                document_id: Some(format!("doc_{}", i)),
                position: Some(i),
                vector_score: Some(0.5),
                bm25_score: None,
                vector_rank: None,
                bm25_rank: None,
                metadata: std::collections::HashMap::new(),
            })
            .collect();

        let json = serde_json::to_string(&large_results).unwrap();
        println!("Large JSON size: {} bytes", json.len());

        // Should fail validation
        let result = ValidatedMetadata::new(json);
        assert!(result.is_err(), "Should reject oversized metadata");

        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("too large"),
            "Error should mention size: {}",
            err_msg
        );
        assert!(
            err_msg.contains("bytes"),
            "Error should show byte count: {}",
            err_msg
        );
    }

    #[test]
    fn test_metadata_size_validation_boundary() {
        // Test at boundary (just under 64KB)
        let num_sources = 50; // Should be safe
        let content_size = 500; // ~500 bytes per source

        let boundary_results: Vec<SearchResultDto> = (0..num_sources)
            .map(|i| SearchResultDto {
                id: format!("chunk_{}", i),
                title: format!("file_{}.txt", i),
                content: "x".repeat(content_size),
                score: 0.7,
                path: Some(format!("/path/to/file_{}.txt", i)),
                document_id: Some(format!("doc_{}", i)),
                position: Some(i),
                vector_score: Some(0.7),
                bm25_score: None,
                vector_rank: None,
                bm25_rank: None,
                metadata: std::collections::HashMap::new(),
            })
            .collect();

        let json = serde_json::to_string(&boundary_results).unwrap();
        println!("Boundary JSON size: {} bytes", json.len());

        // Should succeed if under limit
        if json.len() < ValidatedMetadata::max_size_bytes() {
            let result = ValidatedMetadata::new(json);
            assert!(result.is_ok(), "Should accept boundary-sized metadata");
        }
    }

    #[test]
    fn test_metadata_validation_invalid_json() {
        let invalid_json = "{not valid json}".to_string();
        let result = ValidatedMetadata::new(invalid_json);

        assert!(result.is_err(), "Should reject invalid JSON");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Invalid JSON"),
            "Error should mention JSON: {}",
            err_msg
        );
    }

    #[test]
    fn test_metadata_validation_empty() {
        let empty = "".to_string();
        let result = ValidatedMetadata::new(empty);

        assert!(result.is_err(), "Should reject empty metadata");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("empty"),
            "Error should mention empty: {}",
            err_msg
        );
    }
}
