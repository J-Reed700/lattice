/// Q&A Engine with RAG Pipeline
///
/// A complete RAG (Retrieval-Augmented Generation) implementation that combines
/// semantic search with LLM generation to answer questions using your knowledge base.
use crate::features::qa::QAEngineTrait;
use crate::infrastructure::qa::prompts::{build_user_prompt, SYSTEM_PROMPT};
use crate::infrastructure::qa::tokenizer::{count_tokens, truncate_to_tokens};
use crate::infrastructure::qa::types::{QAError, SourceReference, StreamChunk};
use crate::infrastructure::search::service::SearchResult;
use crate::infrastructure::services::context_manager::LLMContext;
use crate::llm::traits::{ChatMessage, LLMClient};
use async_trait::async_trait;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::Stream;
use tracing::{debug, info, warn};

/// Build chat messages from LLM context and current question
///
/// # Arguments
/// * `ctx` - LLM context with system prompt, document context, and conversation history
/// * `question` - Current user question
///
/// # Returns
/// Array of chat messages for the Chat API
fn build_chat_messages(ctx: &LLMContext, question: &str) -> Result<Vec<ChatMessage>, QAError> {
    let mut messages = Vec::new();

    // System prompt
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: ctx.system_prompt.clone(),
    });

    // Document context (as system message)
    if !ctx.document_context.is_empty() {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: format!("=== RELEVANT CONTEXT ===\n{}", ctx.document_context),
        });
    }

    // Conversation history
    for msg in &ctx.messages {
        messages.push(ChatMessage {
            role: msg.role.clone(),
            content: msg.content.clone(),
        });
    }

    // Current question
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: question.to_string(),
    });

    Ok(messages)
}

/// Q&A engine that combines search and generation
pub struct QAEngine {
    /// LLM client for text generation (supports Ollama, Anthropic, Local)
    llm_client: Arc<dyn LLMClient>,
}

impl QAEngine {
    /// Create a new Q&A engine
    ///
    /// # Arguments
    /// * `llm_client` - LLM client implementing the LLMClient trait
    ///
    /// # Returns
    /// New QAEngine instance
    ///
    /// # Examples
    /// ```no_run
    /// use lattice::qa::QAEngine;
    /// use lattice::llm::{OllamaClient, LLMClient};
    /// use std::sync::Arc;
    ///
    /// let client = Arc::new(OllamaClient::new("http://localhost:11434", "llama3.1:8b".to_string()));
    /// let engine = QAEngine::new(client as Arc<dyn LLMClient>);
    /// ```
    pub fn new(llm_client: Arc<dyn LLMClient>) -> Self {
        Self { llm_client }
    }

    /// Answer a question using the RAG pipeline (non-streaming)
    ///
    /// # Arguments
    /// * `question` - User's question
    /// * `search_results` - Relevant documents from search
    /// * `max_context_tokens` - Maximum tokens for context (default: 2000)
    /// * `llm_context` - Optional conversation context for conversational Q&A
    ///
    /// # Returns
    /// Generated answer
    ///
    /// # Errors
    /// Returns error if generation fails or inputs are invalid
    pub async fn answer(
        &self,
        question: &str,
        search_results: Vec<SearchResult>,
        max_context_tokens: usize,
        llm_context: Option<LLMContext>,
    ) -> Result<String, QAError> {
        // Validate inputs
        if question.trim().is_empty() {
            return Err(QAError::InvalidInput(
                "Question cannot be empty".to_string(),
            ));
        }

        if max_context_tokens == 0 {
            return Err(QAError::InvalidInput(
                "max_context_tokens must be greater than 0".to_string(),
            ));
        }

        info!(
            question = %question,
            num_results = search_results.len(),
            "Processing Q&A request"
        );

        // Check if conversational mode with LLM context
        if let Some(ctx) = llm_context {
            info!(
                num_messages = ctx.messages.len(),
                total_tokens = ctx.total_tokens,
                "Using conversational mode with history"
            );

            // Check if LLM supports chat API
            if self.llm_client.supports_chat() {
                info!("Using Chat API for conversational mode");

                // Build messages array from context
                let messages = build_chat_messages(&ctx, question)?;

                // Use Chat API
                let answer = self.llm_client.generate_chat(messages).await?;
                return Ok(answer);
            } else {
                info!("Chat API not supported, falling back to prompt flattening");

                // Fallback: Flatten messages into prompt
                let prompt = format!("{}\n\nUser Question: {}", ctx.document_context, question);

                let history_text = ctx
                    .messages
                    .iter()
                    .map(|m| format!("{}: {}", m.role, m.content))
                    .collect::<Vec<_>>()
                    .join("\n\n");

                let full_prompt = if !history_text.is_empty() {
                    format!("{}\n\n{}", history_text, prompt)
                } else {
                    prompt
                };

                let answer = self
                    .llm_client
                    .generate(&full_prompt, Some(&ctx.system_prompt), None)
                    .await?;

                return Ok(answer);
            }
        }

        // Non-conversational mode: Existing behavior
        // Handle empty results
        if search_results.is_empty() {
            warn!("No search results for question");
            return Ok(
                "I don't have any information in my knowledge base to answer that question."
                    .to_string(),
            );
        }

        // Build context from search results
        let (context, _sources) = self.build_context(&search_results, max_context_tokens)?;

        debug!(
            context_tokens = count_tokens(&context),
            num_sources = _sources.len(),
            "Built context from search results"
        );

        // Build prompt
        let prompt = build_user_prompt(&context, question);

        let prompt_tokens = count_tokens(SYSTEM_PROMPT) + count_tokens(&prompt);
        info!(
            prompt_tokens = prompt_tokens,
            model = self.llm_client.model_name(),
            "Sending prompt to LLM"
        );

        // Generate answer
        let answer = self
            .llm_client
            .generate(&prompt, Some(SYSTEM_PROMPT), None)
            .await?;

        Ok(answer)
    }

    /// Answer a question with streaming response
    ///
    /// # Arguments
    /// * `question` - User's question
    /// * `search_results` - Relevant documents from search
    /// * `max_context_tokens` - Maximum tokens for context (default: 2000)
    /// * `llm_context` - Optional conversation context for conversational Q&A
    ///
    /// # Returns
    /// Stream of answer chunks and sources
    ///
    /// # Errors
    /// Returns error if generation fails or inputs are invalid
    pub async fn answer_stream(
        &self,
        question: &str,
        search_results: Vec<SearchResult>,
        max_context_tokens: usize,
        llm_context: Option<LLMContext>,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send + '_>>, QAError> {
        // Validate inputs
        if question.trim().is_empty() {
            return Err(QAError::InvalidInput(
                "Question cannot be empty".to_string(),
            ));
        }

        if max_context_tokens == 0 {
            return Err(QAError::InvalidInput(
                "max_context_tokens must be greater than 0".to_string(),
            ));
        }

        info!(
            question = %question,
            num_results = search_results.len(),
            "Processing streaming Q&A request"
        );

        // Check if conversational mode with LLM context
        if let Some(ctx) = llm_context {
            info!(
                num_messages = ctx.messages.len(),
                total_tokens = ctx.total_tokens,
                "Using conversational streaming mode with history"
            );

            // Check if LLM supports chat API
            if self.llm_client.supports_chat() {
                info!("Using Chat API for conversational streaming mode");

                // Build messages array from context
                let messages = build_chat_messages(&ctx, question)?;

                // Use Chat API streaming
                let stream = self.llm_client.generate_chat_stream(messages).await?;

                // Transform stream to include done marker at the end
                let transformed_stream = Box::pin(tokio_stream::StreamExt::chain(
                    futures::stream::StreamExt::map(stream, |result| match result {
                        Ok(content) => StreamChunk::Token { content },
                        Err(e) => StreamChunk::Error {
                            message: e.to_string(),
                        },
                    }),
                    tokio_stream::iter(vec![StreamChunk::Done]),
                ));

                return Ok(transformed_stream);
            } else {
                info!("Chat API not supported, falling back to prompt flattening");

                // Fallback: Flatten messages into prompt
                let prompt = format!("{}\n\nUser Question: {}", ctx.document_context, question);

                let history_text = ctx
                    .messages
                    .iter()
                    .map(|m| format!("{}: {}", m.role, m.content))
                    .collect::<Vec<_>>()
                    .join("\n\n");

                let full_prompt = if !history_text.is_empty() {
                    format!("{}\n\n{}", history_text, prompt)
                } else {
                    prompt
                };

                let stream = self
                    .llm_client
                    .generate_stream(&full_prompt, Some(&ctx.system_prompt), None)
                    .await?;

                let transformed_stream = Box::pin(tokio_stream::StreamExt::chain(
                    futures::stream::StreamExt::map(stream, |result| match result {
                        Ok(content) => StreamChunk::Token { content },
                        Err(e) => StreamChunk::Error {
                            message: e.to_string(),
                        },
                    }),
                    tokio_stream::iter(vec![StreamChunk::Done]),
                ));

                return Ok(transformed_stream);
            }
        }

        // Non-conversational mode: Existing behavior
        // Handle empty results
        if search_results.is_empty() {
            warn!("No search results for question");
            let error_stream = Box::pin(tokio_stream::iter(vec![
                StreamChunk::Token {
                    content:
                        "I don't have any information in my knowledge base to answer that question."
                            .to_string(),
                },
                StreamChunk::Done,
            ]));
            return Ok(error_stream);
        }

        // Build context from search results
        let (context, sources) = self.build_context(&search_results, max_context_tokens)?;

        debug!(
            context_tokens = count_tokens(&context),
            num_sources = sources.len(),
            "Built context from search results"
        );

        // Build prompt
        let prompt = build_user_prompt(&context, question);

        let prompt_tokens = count_tokens(SYSTEM_PROMPT) + count_tokens(&prompt);
        info!(
            prompt_tokens = prompt_tokens,
            model = self.llm_client.model_name(),
            "Starting streaming generation"
        );

        // Get streaming response
        let stream = self
            .llm_client
            .generate_stream(&prompt, Some(SYSTEM_PROMPT), None)
            .await?;

        // Transform stream to include sources at the end
        let transformed_stream = Box::pin(tokio_stream::StreamExt::chain(
            futures::stream::StreamExt::map(stream, |result| match result {
                Ok(content) => StreamChunk::Token { content },
                Err(e) => StreamChunk::Error {
                    message: e.to_string(),
                },
            }),
            tokio_stream::iter(vec![StreamChunk::Sources { sources }, StreamChunk::Done]),
        ));

        Ok(transformed_stream)
    }

    /// Check if LLM client is available
    ///
    /// # Returns
    /// True if the LLM client is reachable and the model is available
    pub async fn health_check(&self) -> bool {
        let available = self.llm_client.health_check().await;

        if available {
            info!(
                model = self.llm_client.model_name(),
                "LLM health check passed"
            );
        } else {
            warn!(
                model = self.llm_client.model_name(),
                "LLM health check failed"
            );
        }

        available
    }

    /// Get the configured model name
    ///
    /// # Returns
    /// Model name string
    pub fn model_name(&self) -> &str {
        self.llm_client.model_name()
    }

    /// Build context string from search results
    ///
    /// Combines search results into a formatted context string, ensuring
    /// the total stays within the token budget.
    ///
    /// # Arguments
    /// * `search_results` - Search results from semantic search
    /// * `max_tokens` - Maximum tokens allowed for entire context
    ///
    /// # Returns
    /// Tuple of (context string, source references)
    fn build_context(
        &self,
        search_results: &[SearchResult],
        max_tokens: usize,
    ) -> Result<(String, Vec<SourceReference>), QAError> {
        let mut context_parts = Vec::new();
        let mut sources = Vec::new();
        let mut current_tokens = 0;

        for result in search_results {
            let file_path = result
                .file_path
                .clone()
                .or_else(|| result.filename.clone())
                .unwrap_or_else(|| format!("Document {}", result.id));

            let score = result.score;
            let content = result.content.clone().unwrap_or_default();

            if content.is_empty() {
                continue;
            }

            // Format document with header
            let doc_header = format!(
                "\n--- Source: {} (relevance: {:.2}) ---\n",
                file_path, score
            );
            let doc_text = format!("{}{}\n", doc_header, content);

            let doc_tokens = count_tokens(&doc_text);

            // Check if adding this document would exceed the budget
            if current_tokens + doc_tokens > max_tokens {
                let remaining_tokens =
                    max_tokens.saturating_sub(current_tokens + count_tokens(&doc_header));

                if remaining_tokens > 100 {
                    // Truncate content to fit
                    let truncated_content =
                        truncate_to_tokens(&content, remaining_tokens, Some("... [truncated]"))?;
                    let doc_text = format!("{}{}\n", doc_header, truncated_content);
                    context_parts.push(doc_text);

                    // Add truncated snippet for source reference
                    let snippet = truncate_to_tokens(&content, 50, Some("..."))?;
                    sources.push(SourceReference {
                        file_path: file_path.clone(),
                        score,
                        snippet,
                    });
                }

                debug!(
                    current_tokens = current_tokens,
                    num_sources = sources.len(),
                    "Reached token limit"
                );
                break;
            } else {
                context_parts.push(doc_text);
                current_tokens += doc_tokens;

                // Create snippet for source reference
                let snippet = truncate_to_tokens(&content, 50, Some("..."))?;
                sources.push(SourceReference {
                    file_path: file_path.clone(),
                    score,
                    snippet,
                });
            }
        }

        let context = if context_parts.is_empty() {
            "No relevant context found.".to_string()
        } else {
            context_parts.join("")
        };

        debug!(
            context_tokens = count_tokens(&context),
            num_sources = sources.len(),
            "Built final context"
        );

        Ok((context, sources))
    }
}

// ============================================================================
// Trait Implementation
// ============================================================================

#[async_trait]
impl QAEngineTrait for QAEngine {
    async fn answer(
        &self,
        question: &str,
        search_results: Vec<SearchResult>,
        max_context_tokens: usize,
        llm_context: Option<LLMContext>,
    ) -> Result<String, QAError> {
        // Delegate to existing implementation
        self.answer(question, search_results, max_context_tokens, llm_context)
            .await
    }

    async fn answer_stream(
        &self,
        question: &str,
        search_results: Vec<SearchResult>,
        max_context_tokens: usize,
        llm_context: Option<LLMContext>,
    ) -> Result<std::pin::Pin<Box<dyn tokio_stream::Stream<Item = StreamChunk> + Send + '_>>, QAError>
    {
        // Get the stream from the existing implementation
        let stream = self
            .answer_stream(question, search_results, max_context_tokens, llm_context)
            .await?;

        // Box and pin it
        Ok(Box::pin(stream))
    }

    async fn health_check(&self) -> bool {
        self.health_check().await
    }

    fn model_name(&self) -> &str {
        self.model_name()
    }
}
