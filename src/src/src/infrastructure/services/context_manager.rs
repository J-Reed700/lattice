//! Context Manager Service
//!
//! Manages the three types of context for conversational RAG:
//! 1. **Conversation History**: User and assistant messages from ConversationAggregate
//! 2. **System Prompt**: Instructions that define LLM behavior
//! 3. **Document Context**: Retrieved chunks from RAG search
//!
//! # Token Budget Management
//!
//! The service allocates tokens across context types:
//! - System prompt: Up to 20% of budget
//! - Document context: Up to 50% of budget
//! - Conversation history: Remaining budget
//!
//! # Example
//!
//! ```rust,no_run
//! use lattice::services::context_manager::ContextManager;
//! use lattice::domain::ConversationAggregate;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = ContextManager::new(4000); // 4000 token budget
//!
//! // Build context for LLM API call
//! let context = manager.build_context_for_llm(
//!     &conversation,
//!     search_results,
//! )?;
//!
//! println!("System: {}", context.system_prompt);
//! println!("Messages: {}", context.messages.len());
//! println!("Documents: {} chars", context.document_context.len());
//! println!("Total tokens: {}", context.total_tokens);
//! # Ok(())
//! # }
//! ```

use crate::domain::conversation::{ConversationAggregate, ConversationMessage, LLMMessage};
use crate::infrastructure::qa::tokenizer::{count_tokens, truncate_to_tokens};
use crate::infrastructure::search::service::SearchResult;
use crate::shared::error::{AppError, Result};

// Default token budget allocation percentages
const SYSTEM_PROMPT_BUDGET_PCT: f32 = 0.20; // 20% for system prompt
const DOCUMENT_CONTEXT_BUDGET_PCT: f32 = 0.50; // 50% for document context
const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful AI assistant with access to relevant documents. Answer the user's question based on the provided context when available.";

// ============================================================================
// LLM Context Output
// ============================================================================

/// Complete context package for LLM API calls
///
/// Contains all three context types formatted and ready for the LLM API.
/// Token counts are tracked to ensure we stay within model limits.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LLMContext {
    /// System prompt with instructions for the LLM
    pub system_prompt: String,

    /// Conversation history formatted as LLM messages
    pub messages: Vec<LLMMessage>,

    /// Document context formatted as a single string
    pub document_context: String,

    /// Total estimated token count across all context types
    pub total_tokens: usize,
}

// ============================================================================
// Context Manager Service
// ============================================================================

/// Service for managing LLM context in conversational RAG
///
/// Handles formatting and token budgeting across three context types:
/// - System prompts
/// - Conversation history
/// - Retrieved documents
///
/// # Token Budget Allocation
///
/// - **20%** for system prompt (max)
/// - **50%** for document context (max)
/// - **Remaining** for conversation history
///
/// If any section is under budget, the remaining tokens are available
/// for other sections.
pub struct ContextManager {
    max_context_tokens: usize,
}

impl ContextManager {
    /// Create a new context manager with specified token budget
    ///
    /// # Arguments
    ///
    /// * `max_context_tokens` - Maximum total tokens across all context types
    ///
    /// # Example
    ///
    /// ```rust
    /// use lattice::services::context_manager::ContextManager;
    ///
    /// // Create manager with 4000 token budget (typical for Claude 3)
    /// let manager = ContextManager::new(4000);
    /// ```
    pub fn new(max_context_tokens: usize) -> Self {
        Self { max_context_tokens }
    }

    /// Get maximum context tokens budget
    ///
    /// # Returns
    ///
    /// Maximum total tokens across all context types
    ///
    /// # Example
    ///
    /// ```rust
    /// use lattice::services::context_manager::ContextManager;
    ///
    /// let manager = ContextManager::new(4000);
    /// assert_eq!(manager.max_context_tokens(), 4000);
    /// ```
    pub fn max_context_tokens(&self) -> usize {
        self.max_context_tokens
    }

    /// Build complete context for LLM API call
    ///
    /// Combines all three context types (system prompt, conversation history,
    /// document context) into a single LLMContext ready for the API.
    ///
    /// # Token Budget Allocation
    ///
    /// 1. System prompt gets up to 20% of budget
    /// 2. Document context gets up to 50% of budget
    /// 3. Conversation history gets remaining budget
    /// 4. If any section uses less, others can expand
    ///
    /// # Arguments
    ///
    /// * `conversation` - Conversation aggregate with history and system prompt
    /// * `search_results` - RAG search results to include as context
    ///
    /// # Returns
    ///
    /// LLMContext with all sections formatted and token-counted
    ///
    /// # Errors
    ///
    /// Returns error if token budget is insufficient for minimum context
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::services::context_manager::ContextManager;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = ContextManager::new(4000);
    /// let context = manager.build_context_for_llm(
    ///     &conversation,
    ///     search_results,
    /// )?;
    ///
    /// // Use context in LLM API call
    /// let response = llm_client.send(
    ///     &context.system_prompt,
    ///     &context.messages,
    ///     Some(&context.document_context),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn build_context_for_llm(
        &self,
        conversation: &ConversationAggregate,
        search_results: Vec<SearchResult>,
    ) -> Result<LLMContext> {
        // Calculate token budgets for each section
        let system_budget = (self.max_context_tokens as f32 * SYSTEM_PROMPT_BUDGET_PCT) as usize;
        let document_budget =
            (self.max_context_tokens as f32 * DOCUMENT_CONTEXT_BUDGET_PCT) as usize;

        // Format system prompt
        let system_prompt = Self::format_system_context(conversation.system_prompt());
        let system_tokens = count_tokens(&system_prompt);

        // Format document context with budget
        let document_context = Self::format_document_context(search_results, document_budget)?;
        let document_tokens = count_tokens(&document_context);

        // Calculate remaining budget for conversation history
        let used_tokens = system_tokens + document_tokens;
        let conversation_budget = self.max_context_tokens.saturating_sub(used_tokens);

        // Format conversation history
        let messages = Self::format_conversation_history(conversation.messages());

        // Estimate tokens in messages and truncate if needed
        let messages_tokens: usize = messages.iter().map(|m| count_tokens(&m.content)).sum();

        let final_messages = if messages_tokens > conversation_budget {
            // Truncate oldest messages to fit budget
            Self::truncate_messages(messages, conversation_budget)
        } else {
            messages
        };

        let total_tokens = system_tokens
            + document_tokens
            + count_tokens(
                &final_messages
                    .iter()
                    .map(|m| m.content.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            );

        Ok(LLMContext {
            system_prompt,
            messages: final_messages,
            document_context,
            total_tokens,
        })
    }

    /// Format system context (system prompt)
    ///
    /// Provides a default system prompt if none is specified in the conversation.
    ///
    /// # Arguments
    ///
    /// * `system_prompt` - Optional custom system prompt from conversation
    ///
    /// # Returns
    ///
    /// Formatted system prompt string
    ///
    /// # Example
    ///
    /// ```rust
    /// use lattice::services::context_manager::ContextManager;
    ///
    /// let default = ContextManager::format_system_context(None);
    /// assert!(default.contains("helpful AI assistant"));
    ///
    /// let custom = ContextManager::format_system_context(
    ///     Some("You are a coding assistant.")
    /// );
    /// assert_eq!(custom, "You are a coding assistant.");
    /// ```
    pub fn format_system_context(system_prompt: Option<&str>) -> String {
        system_prompt
            .map(|s| s.to_string())
            .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string())
    }

    /// Format document context from search results
    ///
    /// Formats retrieved documents with headers, metadata, and content.
    /// Truncates if needed to fit within token budget.
    ///
    /// # Format
    ///
    /// ```text
    /// === RELEVANT DOCUMENTS ===
    ///
    /// Document 1 (relevance: 0.95):
    /// Source: document.txt
    /// ---
    /// [content]
    ///
    /// Document 2 (relevance: 0.87):
    /// Source: another.pdf
    /// ---
    /// [content]
    /// ```
    ///
    /// # Arguments
    ///
    /// * `search_results` - Search results to format
    /// * `max_tokens` - Maximum tokens for document context
    ///
    /// # Returns
    ///
    /// Formatted document context string
    ///
    /// # Errors
    ///
    /// Returns error if truncation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::services::context_manager::ContextManager;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let context = ContextManager::format_document_context(
    ///     search_results,
    ///     2000, // 2000 token budget
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn format_document_context(
        search_results: Vec<SearchResult>,
        max_tokens: usize,
    ) -> Result<String> {
        if search_results.is_empty() {
            return Ok(String::new());
        }

        let mut context = String::from("=== RELEVANT DOCUMENTS ===\n\n");

        for (idx, result) in search_results.iter().enumerate() {
            // Document header
            context.push_str(&format!(
                "Document {} (relevance: {:.2}):\n",
                idx + 1,
                result.score
            ));

            // Metadata
            if let Some(ref filename) = result.filename {
                context.push_str(&format!("Source: {}\n", filename));
            } else if let Some(ref file_name) = result.file_name {
                context.push_str(&format!("Source: {}\n", file_name));
            }

            if let Some(ref mime_type) = result.mime_type {
                context.push_str(&format!("Type: {}\n", mime_type));
            }

            context.push_str("---\n");

            // Content
            if let Some(ref content) = result.content {
                context.push_str(content);
                context.push_str("\n\n");
            }
        }

        // Truncate to fit budget if needed
        let current_tokens = count_tokens(&context);
        if current_tokens > max_tokens {
            truncate_to_tokens(&context, max_tokens, Some("\n\n[...truncated...]"))
                .map_err(|e| AppError::Other(e.to_string()))
        } else {
            Ok(context)
        }
    }

    /// Format conversation history as LLM messages
    ///
    /// Converts ConversationMessage objects to LLMMessage format
    /// compatible with Anthropic Messages API.
    ///
    /// # Arguments
    ///
    /// * `messages` - Conversation messages to format
    ///
    /// # Returns
    ///
    /// Vector of LLMMessage objects
    ///
    /// # Example
    ///
    /// ```rust
    /// use lattice::services::context_manager::ContextManager;
    ///
    /// let messages = ContextManager::format_conversation_history(&conversation.messages());
    /// for msg in messages {
    ///     println!("{}: {}", msg.role, msg.content);
    /// }
    /// ```
    pub fn format_conversation_history(messages: &[ConversationMessage]) -> Vec<LLMMessage> {
        messages
            .iter()
            .map(|msg| LLMMessage {
                role: match msg.role {
                    crate::domain::conversation::MessageRole::User => "user".to_string(),
                    crate::domain::conversation::MessageRole::Assistant => "assistant".to_string(),
                    crate::domain::conversation::MessageRole::System => "system".to_string(),
                },
                content: msg.content.clone(),
            })
            .collect()
    }

    /// Truncate messages to fit within token budget
    ///
    /// Removes oldest messages first, preserving most recent conversation context.
    /// Always keeps at least the last message.
    ///
    /// # Arguments
    ///
    /// * `messages` - Messages to truncate
    /// * `max_tokens` - Maximum token budget
    ///
    /// # Returns
    ///
    /// Truncated messages that fit within budget
    fn truncate_messages(messages: Vec<LLMMessage>, max_tokens: usize) -> Vec<LLMMessage> {
        if messages.is_empty() {
            return vec![];
        }

        let mut cumulative_tokens = 0;
        let mut keep_from_index = messages.len();

        // Count tokens from most recent to oldest
        for (i, message) in messages.iter().enumerate().rev() {
            let msg_tokens = count_tokens(&message.content);
            cumulative_tokens += msg_tokens;

            if cumulative_tokens > max_tokens {
                keep_from_index = i + 1;
                break;
            }
        }

        // If keep_from_index is still at messages.len(), all messages fit
        if keep_from_index == messages.len() {
            return messages;
        }

        // Otherwise, ensure we keep at least the last message
        keep_from_index = keep_from_index.min(messages.len() - 1);

        messages
            .get(keep_from_index..)
            .map(|slice| slice.to_vec())
            .unwrap_or_else(|| messages.to_vec())
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        // Default to 4000 tokens (conservative for most models)
        Self::new(4000)
    }
}

// ============================================================================
// Trait Implementation
// ============================================================================

use crate::infrastructure::services::traits::ContextManagerTrait;

impl ContextManagerTrait for ContextManager {
    fn build_context_for_llm(
        &self,
        conversation: &crate::domain::conversation::ConversationAggregate,
        search_results: Vec<SearchResult>,
    ) -> Result<LLMContext> {
        ContextManager::build_context_for_llm(self, conversation, search_results)
    }

    fn format_system_context(&self, system_prompt: Option<&str>) -> String {
        ContextManager::format_system_context(system_prompt)
    }

    fn format_document_context(
        &self,
        search_results: Vec<SearchResult>,
        max_tokens: usize,
    ) -> Result<String> {
        ContextManager::format_document_context(search_results, max_tokens)
    }

    fn format_conversation_history(
        &self,
        messages: &[crate::domain::conversation::ConversationMessage],
    ) -> Vec<crate::domain::conversation::LLMMessage> {
        ContextManager::format_conversation_history(messages)
    }

    fn max_context_tokens(&self) -> usize {
        self.max_context_tokens()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::conversation::MessageRole;

    fn create_test_conversation() -> ConversationAggregate {
        let mut conv = ConversationAggregate::new(
            "Test Conversation".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            Some("You are a test assistant.".to_string()),
        )
        .unwrap();

        conv.add_message(MessageRole::User, "What is RAG?".to_string(), 50)
            .unwrap();

        conv.add_message(
            MessageRole::Assistant,
            "RAG stands for Retrieval-Augmented Generation...".to_string(),
            200,
        )
        .unwrap();

        conv
    }

    fn create_test_search_results() -> Vec<SearchResult> {
        vec![
            SearchResult {
                id: "chunk1".to_string(),
                score: 0.95,
                index: 0,
                filename: Some("doc1.txt".to_string()),
                mime_type: Some("text/plain".to_string()),
                size_bytes: Some(1024),
                created_at: Some("2024-01-01".to_string()),
                content: Some("This is the content of document 1 about RAG systems.".to_string()),
                file_id: Some("file1".to_string()),
                file_path: Some("/path/to/doc1.txt".to_string()),
                file_name: Some("doc1.txt".to_string()),
                file_extension: Some("txt".to_string()),
                file_category: Some("text".to_string()),
                is_indexed: Some(true),
                document_id: Some("doc1".to_string()),
                snippet: Some("RAG systems".to_string()),
                chunk_index: Some(0),
                updated_at: Some("2024-01-01".to_string()),
            },
            SearchResult {
                id: "chunk2".to_string(),
                score: 0.87,
                index: 1,
                filename: Some("doc2.pdf".to_string()),
                mime_type: Some("application/pdf".to_string()),
                size_bytes: Some(2048),
                created_at: Some("2024-01-02".to_string()),
                content: Some(
                    "Document 2 contains information about vector embeddings.".to_string(),
                ),
                file_id: Some("file2".to_string()),
                file_path: Some("/path/to/doc2.pdf".to_string()),
                file_name: Some("doc2.pdf".to_string()),
                file_extension: Some("pdf".to_string()),
                file_category: Some("document".to_string()),
                is_indexed: Some(true),
                document_id: Some("doc2".to_string()),
                snippet: Some("vector embeddings".to_string()),
                chunk_index: Some(0),
                updated_at: Some("2024-01-02".to_string()),
            },
        ]
    }

    #[test]
    fn test_new_context_manager() {
        let manager = ContextManager::new(4000);
        assert_eq!(manager.max_context_tokens, 4000);
    }

    #[test]
    fn test_default_context_manager() {
        let manager = ContextManager::default();
        assert_eq!(manager.max_context_tokens, 4000);
    }

    #[test]
    fn test_format_system_context_default() {
        let prompt = ContextManager::format_system_context(None);
        assert!(prompt.contains("helpful AI assistant"));
        assert!(prompt.contains("relevant documents"));
    }

    #[test]
    fn test_format_system_context_custom() {
        let custom = "You are a coding assistant.";
        let prompt = ContextManager::format_system_context(Some(custom));
        assert_eq!(prompt, custom);
    }

    #[test]
    fn test_format_document_context_empty() {
        let context = ContextManager::format_document_context(vec![], 1000).unwrap();
        assert_eq!(context, "");
    }

    #[test]
    fn test_format_document_context_with_results() {
        let results = create_test_search_results();
        let context = ContextManager::format_document_context(results, 2000).unwrap();

        assert!(context.contains("=== RELEVANT DOCUMENTS ==="));
        assert!(context.contains("Document 1 (relevance: 0.95)"));
        assert!(context.contains("Document 2 (relevance: 0.87)"));
        assert!(context.contains("Source: doc1.txt"));
        assert!(context.contains("Source: doc2.pdf"));
        assert!(context.contains("RAG systems"));
        assert!(context.contains("vector embeddings"));
    }

    #[test]
    fn test_format_document_context_truncation() {
        let results = create_test_search_results();
        // Very small budget should trigger truncation
        let context = ContextManager::format_document_context(results, 50).unwrap();

        assert!(context.contains("=== RELEVANT DOCUMENTS ==="));
        assert!(context.contains("[...truncated...]"));
    }

    #[test]
    fn test_format_conversation_history() {
        let conv = create_test_conversation();
        let messages = ContextManager::format_conversation_history(conv.messages());

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "What is RAG?");
        assert_eq!(messages[1].role, "assistant");
        assert!(messages[1]
            .content
            .contains("Retrieval-Augmented Generation"));
    }

    #[test]
    fn test_truncate_messages_no_truncation_needed() {
        let messages = vec![
            LLMMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            },
            LLMMessage {
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
            },
        ];

        let truncated = ContextManager::truncate_messages(messages.clone(), 1000);
        assert_eq!(truncated.len(), 2);
        assert_eq!(truncated[0].content, "Hello");
    }

    #[test]
    fn test_truncate_messages_removes_oldest() {
        let messages = vec![
            LLMMessage {
                role: "user".to_string(),
                content: "First message with some content".to_string(),
            },
            LLMMessage {
                role: "assistant".to_string(),
                content: "Second message".to_string(),
            },
            LLMMessage {
                role: "user".to_string(),
                content: "Third message".to_string(),
            },
        ];

        // Small budget should keep only recent messages
        let truncated = ContextManager::truncate_messages(messages, 10);
        assert!(truncated.len() < 3);
        // Should keep at least the last message
        assert!(!truncated.is_empty());
        assert_eq!(truncated.last().unwrap().content, "Third message");
    }

    #[test]
    fn test_truncate_messages_keeps_at_least_one() {
        let messages = vec![LLMMessage {
            role: "user".to_string(),
            content: "Very long message that exceeds budget...".repeat(100),
        }];

        // Even with budget of 1, should keep the last message
        let truncated = ContextManager::truncate_messages(messages, 1);
        assert_eq!(truncated.len(), 1);
    }

    #[test]
    fn test_build_context_for_llm() {
        let manager = ContextManager::new(4000);
        let conv = create_test_conversation();
        let results = create_test_search_results();

        let context = manager.build_context_for_llm(&conv, results).unwrap();

        // System prompt should be present
        assert!(!context.system_prompt.is_empty());
        assert_eq!(context.system_prompt, "You are a test assistant.");

        // Messages should be formatted
        assert_eq!(context.messages.len(), 2);
        assert_eq!(context.messages[0].role, "user");
        assert_eq!(context.messages[1].role, "assistant");

        // Document context should be present
        assert!(!context.document_context.is_empty());
        assert!(context
            .document_context
            .contains("=== RELEVANT DOCUMENTS ==="));

        // Total tokens should be tracked
        assert!(context.total_tokens > 0);
        assert!(context.total_tokens <= 4000);
    }

    #[test]
    fn test_build_context_no_search_results() {
        let manager = ContextManager::new(4000);
        let conv = create_test_conversation();

        let context = manager.build_context_for_llm(&conv, vec![]).unwrap();

        assert!(!context.system_prompt.is_empty());
        assert_eq!(context.messages.len(), 2);
        assert_eq!(context.document_context, "");
        assert!(context.total_tokens > 0);
    }

    #[test]
    fn test_build_context_token_budget_enforcement() {
        // Very small budget
        let manager = ContextManager::new(100);
        let conv = create_test_conversation();
        let results = create_test_search_results();

        let context = manager.build_context_for_llm(&conv, results).unwrap();

        // Should still work but with truncation
        assert!(!context.system_prompt.is_empty());
        assert!(!context.messages.is_empty());
        // Total should respect budget (with some margin for approximation)
        assert!(context.total_tokens <= 150); // Allow some margin
    }

    #[test]
    fn test_format_conversation_history_empty() {
        let messages = ContextManager::format_conversation_history(&[]);
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_format_conversation_history_system_role() {
        let conv_messages = vec![ConversationMessage {
            id: "1".to_string(),
            conversation_id: crate::domain_types::ConversationId::new(),
            role: MessageRole::System,
            content: "System message".to_string(),
            tokens: 10,
            created_at: chrono::Utc::now(),
            metadata: None,
            status: "completed".to_string(),
        }];

        let messages = ContextManager::format_conversation_history(&conv_messages);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "system");
    }

    #[test]
    fn test_token_budget_allocation() {
        let manager = ContextManager::new(1000);

        // System budget should be ~20% = 200 tokens
        let system_budget = (1000.0 * SYSTEM_PROMPT_BUDGET_PCT) as usize;
        assert_eq!(system_budget, 200);

        // Document budget should be ~50% = 500 tokens
        let document_budget = (1000.0 * DOCUMENT_CONTEXT_BUDGET_PCT) as usize;
        assert_eq!(document_budget, 500);

        // Remaining for conversation ~30% = 300 tokens
        let remaining = 1000 - system_budget - document_budget;
        assert_eq!(remaining, 300);
    }
}
