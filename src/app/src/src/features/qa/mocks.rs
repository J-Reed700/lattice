//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::infrastructure::search::service::SearchResult;
#[cfg(test)]
use super::traits::{ConversationalQAServiceTrait, QAEngineTrait};
#[cfg(test)]
use crate::features::conversation::ConversationServiceTrait;
#[cfg(test)]
use crate::shared::error::Result;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, Mutex};
#[cfg(test)]
use tauri::Emitter;
#[cfg(test)]
use tokio::sync::RwLock;

#[cfg(test)]
/// Mock Q&A engine for testing
///
/// Simulates Q&A operations without actual LLM calls.
/// Returns configurable mock answers for testing Q&A flow.
/// All operations are deterministic and thread-safe.
pub struct MockQAEngine {
    /// Mock answers to return (question -> answer mapping)
    answers: Arc<RwLock<std::collections::HashMap<String, String>>>,
    /// Model name to report
    model_name: String,
    /// Health check status
    is_healthy: Arc<RwLock<bool>>,
}

#[cfg(test)]
impl MockQAEngine {
    /// Create new mock engine with default answers
    ///
    /// # Returns
    /// New mock engine with default model name and healthy status
    pub fn new() -> Self {
        let mut answers = std::collections::HashMap::new();
        answers.insert(
            "*".to_string(),
            "This is a mock answer for testing.".to_string(),
        );

        Self {
            answers: Arc::new(RwLock::new(answers)),
            model_name: "mock-model".to_string(),
            is_healthy: Arc::new(RwLock::new(true)),
        }
    }

    /// Create mock with custom model name
    ///
    /// # Arguments
    /// * `model_name` - Model name to report
    ///
    /// # Returns
    /// New mock engine with custom model name
    pub fn with_model_name(model_name: String) -> Self {
        let mut engine = Self::new();
        engine.model_name = model_name;
        engine
    }

    /// Set answer for a specific question
    ///
    /// When `answer` is called with this question, the configured answer will be returned.
    /// Use "*" as question for a default/wildcard answer.
    ///
    /// # Arguments
    /// * `question` - The question to match
    /// * `answer` - The answer to return
    ///
    /// # Example
    /// ```rust
    /// let mock = MockQAEngine::new();
    /// mock.set_answer("What is Rust?", "Rust is a systems programming language.");
    /// mock.set_answer("*", "Default answer for any question");
    /// ```
    pub async fn set_answer(&self, question: &str, answer: String) {
        self.answers
            .write()
            .await
            .insert(question.to_string(), answer);
    }

    /// Set multiple answers at once
    ///
    /// # Arguments
    /// * `answers` - Map of question -> answer pairs
    pub async fn set_answers(&self, answers: std::collections::HashMap<String, String>) {
        *self.answers.write().await = answers;
    }

    /// Clear all configured answers
    pub async fn clear_answers(&self) {
        self.answers.write().await.clear();
    }

    /// Set health check status
    ///
    /// # Arguments
    /// * `is_healthy` - Whether health checks should return true
    pub async fn set_healthy(&self, is_healthy: bool) {
        *self.is_healthy.write().await = is_healthy;
    }
}

#[cfg(test)]
impl Default for MockQAEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl QAEngineTrait for MockQAEngine {
    async fn answer(
        &self,
        question: &str,
        _search_results: Vec<SearchResult>,
        _max_context_tokens: usize,
        _llm_context: Option<crate::infrastructure::services::context_manager::LLMContext>,
    ) -> Result<String, crate::infrastructure::qa::types::QAError> {
        // Validate inputs
        if question.trim().is_empty() {
            return Err(crate::infrastructure::qa::types::QAError::InvalidInput(
                "Question cannot be empty".to_string(),
            ));
        }

        // Get mock answer
        let answers = self.answers.read().await;
        let answer = answers
            .get(question)
            .or_else(|| answers.get("*"))
            .cloned()
            .unwrap_or_else(|| "Mock answer not configured".to_string());

        Ok(answer)
    }

    async fn answer_stream(
        &self,
        question: &str,
        search_results: Vec<SearchResult>,
        max_context_tokens: usize,
        llm_context: Option<crate::infrastructure::services::context_manager::LLMContext>,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn tokio_stream::Stream<Item = crate::infrastructure::qa::types::StreamChunk>
                    + Send
                    + '_,
            >,
        >,
        crate::infrastructure::qa::types::QAError,
    > {
        use crate::infrastructure::qa::types::StreamChunk;

        // Validate inputs
        if question.trim().is_empty() {
            return Err(crate::infrastructure::qa::types::QAError::InvalidInput(
                "Question cannot be empty".to_string(),
            ));
        }

        // Get answer first
        let answer = self
            .answer(question, search_results, max_context_tokens, llm_context)
            .await?;

        // Split answer into words for streaming simulation
        let words: Vec<String> = answer
            .split_whitespace()
            .map(|w| format!("{} ", w))
            .collect();

        // Create stream chunks
        let mut chunks: Vec<StreamChunk> = words
            .into_iter()
            .map(|content| StreamChunk::Token { content })
            .collect();

        // Add sources and done
        chunks.push(StreamChunk::Sources { sources: vec![] });
        chunks.push(StreamChunk::Done);

        // Convert to stream
        let stream = tokio_stream::iter(chunks);
        Ok(Box::pin(stream))
    }

    async fn health_check(&self) -> bool {
        *self.is_healthy.read().await
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}

#[cfg(test)]
/// Mock conversational Q&A service for testing
///
/// Simulates conversational Q&A without actual LLM calls.
/// Returns configurable mock answers for testing conversational flow.
/// All operations are deterministic and thread-safe.
pub struct MockConversationalQAService {
    /// Conversation service for persistence
    conversation_service: Arc<dyn ConversationServiceTrait>,
    /// Mock answers to return (question -> answer mapping)
    mock_answers: Arc<RwLock<std::collections::HashMap<String, String>>>,
}

#[cfg(test)]
impl MockConversationalQAService {
    /// Create new mock service
    ///
    /// # Arguments
    /// * `conversation_service` - Conversation service for message storage
    ///
    /// # Returns
    /// New mock service with default mock answer
    pub fn new(conversation_service: Arc<dyn ConversationServiceTrait>) -> Self {
        let mut answers = std::collections::HashMap::new();
        answers.insert(
            "*".to_string(),
            "This is a mock answer for testing.".to_string(),
        );

        Self {
            conversation_service,
            mock_answers: Arc::new(RwLock::new(answers)),
        }
    }

    /// Configure mock answer for a specific question
    ///
    /// # Arguments
    /// * `question` - Question pattern to match (use "*" for all questions)
    /// * `answer` - Mock answer to return
    ///
    /// # Example
    /// ```rust
    /// mock.set_answer("What is ML?", "Machine Learning is...".to_string());
    /// ```
    pub async fn set_answer(&self, question: &str, answer: String) {
        self.mock_answers
            .write()
            .await
            .insert(question.to_string(), answer);
    }

    /// Clear all configured mock answers
    pub async fn clear(&self) {
        self.mock_answers.write().await.clear();
    }
}

#[async_trait]
#[cfg(test)]
impl ConversationalQAServiceTrait for MockConversationalQAService {
    async fn ask_question(
        &self,
        conversation_id: &str,
        question: &str,
        search_results: Vec<crate::features::search::dto::SearchResultDto>,
    ) -> Result<crate::features::qa::conversational_service::ConversationalAnswer> {
        // Load conversation to verify it exists
        let _aggregate = self
            .conversation_service
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!(
                    "Conversation not found: {}",
                    conversation_id
                ))
            })?;

        // Get mock answer
        let answers = self.mock_answers.read().await;
        let answer = answers
            .get(question)
            .or_else(|| answers.get("*"))
            .cloned()
            .unwrap_or_else(|| "Mock answer not configured".to_string());

        // Estimate tokens
        let question_tokens = (question.len() / 4) as i64;
        let answer_tokens = (answer.len() / 4) as i64;

        // Save messages
        self.conversation_service
            .add_user_message(conversation_id, question.to_string(), question_tokens)
            .await?;

        self.conversation_service
            .add_assistant_message(conversation_id, answer.clone(), answer_tokens)
            .await?;

        // Get updated conversation
        let updated_aggregate = self
            .conversation_service
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!(
                    "Conversation not found: {}",
                    conversation_id
                ))
            })?;

        Ok(
            crate::features::qa::conversational_service::ConversationalAnswer {
                answer,
                sources: search_results,
                conversation_id: conversation_id.to_string(),
                message_count: updated_aggregate.message_count(),
                total_tokens: updated_aggregate.total_tokens(),
            },
        )
    }

    async fn ask_question_stream(
        &self,
        conversation_id: &str,
        question: &str,
        search_results: Vec<crate::features::search::dto::SearchResultDto>,
        window: tauri::Window,
    ) -> Result<()> {
        use crate::infrastructure::qa::types::StreamChunk;

        // Load conversation to verify it exists
        let _aggregate = self
            .conversation_service
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!(
                    "Conversation not found: {}",
                    conversation_id
                ))
            })?;

        // Get mock answer
        let answers = self.mock_answers.read().await;
        let answer = answers
            .get(question)
            .or_else(|| answers.get("*"))
            .cloned()
            .unwrap_or_else(|| "Mock answer not configured".to_string());

        // Emit streaming chunks (split answer into words for realistic streaming)
        for word in answer.split_whitespace() {
            let chunk = StreamChunk::Token {
                content: format!("{} ", word),
            };
            window
                .emit_to(window.label(), "llm-stream", &chunk)
                .map_err(|e| crate::error::AppError::Other(format!("Failed to emit: {}", e)))?;
        }

        // Emit done
        window
            .emit_to(window.label(), "llm-stream", &StreamChunk::Done)
            .map_err(|e| crate::error::AppError::Other(format!("Failed to emit: {}", e)))?;

        // Estimate tokens
        let question_tokens = (question.len() / 4) as i64;
        let answer_tokens = (answer.len() / 4) as i64;

        // Save messages
        self.conversation_service
            .add_user_message(conversation_id, question.to_string(), question_tokens)
            .await?;

        self.conversation_service
            .add_assistant_message(conversation_id, answer.clone(), answer_tokens)
            .await?;

        Ok(())
    }
}

// ============================================================================
// Indexing Service Trait
// ============================================================================
