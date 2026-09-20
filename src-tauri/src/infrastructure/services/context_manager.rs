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

use crate::application::services::context_assembler::{
    BudgetAllocation, ModelCapacity, MESSAGE_FRAMING_TOKENS,
};
use crate::domain::conversation::{ConversationAggregate, ConversationMessage, LLMMessage};
use crate::features::qa::engine::tokenizer::{count_tokens, truncate_to_tokens};
use crate::features::search::engine::service::SearchResult;
use crate::shared::error::{AppError, Result};

// Default token budget allocation percentages
/// Retained only as the fallback when the shared allocator refuses a capacity
/// (a model too small to plan for). The live policy is
/// [`BudgetAllocation`], so this path and the chat path agree.
const DOCUMENT_CONTEXT_BUDGET_PCT: f32 = 0.50;
const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful AI assistant with access to relevant documents. Answer the user's question based on the provided context when available.";

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
        // Format system prompt
        let system_prompt = Self::format_system_context(conversation.system_prompt());
        let system_tokens = count_tokens(&system_prompt);

        // Budgeted by the same rules the chat path uses, rather than this
        // module's own percentages. Two budget policies for one model is how a
        // prompt that fits on one path overruns on the other.
        let allocation = BudgetAllocation::plan(
            &ModelCapacity::new("conversational-qa", self.max_context_tokens),
            system_tokens.min(self.max_context_tokens),
        )
        .ok();
        let document_budget = allocation
            .as_ref()
            .map(|allocation| allocation.rag_and_tools)
            .unwrap_or_else(|| {
                (self.max_context_tokens as f32 * DOCUMENT_CONTEXT_BUDGET_PCT) as usize
            });

        // Format document context with budget
        let document_context = Self::format_document_context(search_results, document_budget)?;
        let document_tokens = count_tokens(&document_context);

        let used_tokens = system_tokens + document_tokens;
        let conversation_budget = allocation
            .as_ref()
            .map(|allocation| allocation.recent_history.max(allocation.available / 4))
            .unwrap_or_else(|| self.max_context_tokens.saturating_sub(used_tokens))
            .min(self.max_context_tokens.saturating_sub(used_tokens));

        // `live_messages`, not `messages`: with a compaction active the folded
        // prefix is represented by the summary, and replaying it here sent the
        // whole archive *and* grew with it. The summary goes in front of the
        // messages it stands for, the same order the chat path renders.
        let mut messages = Vec::new();
        if let Some(preamble) = conversation.context_preamble() {
            messages.push(LLMMessage {
                role: "assistant".to_string(),
                content: preamble,
            });
        }
        messages.extend(Self::format_conversation_history(
            conversation.live_messages(),
        ));

        // Estimate tokens in messages and truncate if needed
        let messages_tokens: usize = messages.iter().map(|m| count_tokens(&m.content)).sum();

        let final_messages = if messages_tokens > conversation_budget {
            Self::evict_history_to_budget(messages, conversation_budget)
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

    /// Drop history until it fits, in the order the chat assembler uses.
    ///
    /// The order is the point, not an implementation detail. `context_assembler`
    /// evicts document evidence, then recalled passages, then the summary, then
    /// the oldest processed turns; this prompt carries only the last two of
    /// those, so it gives up the generated summary before it gives up any of the
    /// user's own words. Two paths that discard different things under the same
    /// pressure answer the same question differently, and that divergence is
    /// exactly what a single rule removes.
    ///
    /// Charges [`MESSAGE_FRAMING_TOKENS`] per message for the same reason the
    /// assembler does: every provider spends tokens on role delimiters, and
    /// counting zero for them is what makes a prompt that "fits" come back
    /// rejected by the API.
    ///
    /// A turn is dropped whole. Removing a question and leaving its answer
    /// behind produces a transcript where the assistant appears to have
    /// volunteered something unprompted, which is worse than having less
    /// history. The newest message is never dropped.
    fn evict_history_to_budget(messages: Vec<LLMMessage>, max_tokens: usize) -> Vec<LLMMessage> {
        fn charge(message: &LLMMessage) -> usize {
            count_tokens(&message.content) + MESSAGE_FRAMING_TOKENS
        }
        fn total(messages: &[LLMMessage]) -> usize {
            messages.iter().map(charge).sum()
        }

        let mut messages = messages;
        if messages.is_empty() || total(&messages) <= max_tokens {
            return messages;
        }

        // The generated summary goes first. It is the only thing here that no
        // user wrote, so it is the only thing whose loss costs no original words.
        if let Some(index) = messages
            .iter()
            .position(|message| message.content.starts_with("[generated summary"))
        {
            messages.remove(index);
        }

        // Then the oldest turns, whole, while anything but the newest remains.
        while total(&messages) > max_tokens && messages.len() > 1 {
            let pair = messages.len() > 2
                && messages.first().is_some_and(|first| first.role == "user")
                && messages
                    .get(1)
                    .is_some_and(|second| second.role == "assistant");
            let drop = if pair { 2 } else { 1 };
            messages.drain(..drop.min(messages.len() - 1));
        }

        messages
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        // Default to 4000 tokens (conservative for most models)
        Self::new(4000)
    }
}

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

    /// Small helper so the eviction tests read as the sequences they describe.
    fn history(turns: &[(&str, &str)]) -> Vec<LLMMessage> {
        turns
            .iter()
            .map(|(role, content)| LLMMessage {
                role: (*role).to_string(),
                content: (*content).to_string(),
            })
            .collect()
    }

    #[test]
    fn history_that_already_fits_is_left_exactly_as_it_was() {
        let messages = history(&[("user", "Hello"), ("assistant", "Hi there!")]);

        let kept = ContextManager::evict_history_to_budget(messages.clone(), 1000);

        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].content, "Hello");
    }

    #[test]
    fn the_generated_summary_is_given_up_before_any_of_the_users_own_words() {
        let summary = crate::domain::conversation_memory::frame_generated_summary(
            "The user chose Postgres and asked for nightly backups.",
        );
        let messages = history(&[
            ("assistant", summary.as_str()),
            ("user", "And what about the retention window?"),
        ]);

        // A budget that fits the question but not both.
        let budget = count_tokens("And what about the retention window?") + MESSAGE_FRAMING_TOKENS;
        let kept = ContextManager::evict_history_to_budget(messages, budget);

        // The assembler's eviction order, and the reason for it: the summary is
        // the only entry here that no user wrote.
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].content, "And what about the retention window?");
    }

    #[test]
    fn a_turn_is_dropped_whole_so_an_answer_never_outlives_its_question() {
        let messages = history(&[
            ("user", "Which database should we use?"),
            ("assistant", "Postgres, for the constraints you described."),
            ("user", "And the retention window?"),
        ]);

        let budget = count_tokens("And the retention window?") + MESSAGE_FRAMING_TOKENS;
        let kept = ContextManager::evict_history_to_budget(messages, budget);

        assert_eq!(
            kept.len(),
            1,
            "the question and its answer go together or not at all"
        );
        assert_eq!(kept[0].content, "And the retention window?");
    }

    #[test]
    fn the_newest_message_survives_a_budget_it_cannot_possibly_fit() {
        let messages = history(&[("user", "placeholder")]);
        let mut messages = messages;
        messages[0].content = "Very long message that exceeds budget...".repeat(100);

        // Dropping it would send a prompt with no question in it at all, which
        // fails less usefully than an over-budget request the provider rejects.
        let kept = ContextManager::evict_history_to_budget(messages, 1);

        assert_eq!(kept.len(), 1);
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
            conversation_id: crate::shared::domain_types::ConversationId::new(),
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
    fn qa_budgets_from_the_same_allocator_the_chat_path_uses() {
        // The previous test asserted arithmetic on this module's own percentage
        // constants, which proved nothing about behaviour and drifted from the
        // chat path for free. What matters is that one policy governs both.
        let manager = ContextManager::new(32_768);
        let conversation = aggregate_with("Answer well.", 4);
        let context = manager
            .build_context_for_llm(&conversation, Vec::new())
            .expect("context builds");

        let allocation = BudgetAllocation::plan(
            &ModelCapacity::new("conversational-qa", 32_768),
            count_tokens(&context.system_prompt),
        )
        .expect("the shared allocator plans this capacity");
        assert!(
            context.total_tokens <= allocation.input_budget,
            "QA spent {} against an input budget of {}",
            context.total_tokens,
            allocation.input_budget
        );
    }

    #[test]
    fn qa_carries_the_summary_instead_of_replaying_a_compacted_prefix() {
        let manager = ContextManager::new(32_768);
        let plain = aggregate_with("Answer well.", 6);
        let boundary = plain.messages()[3].id.clone();

        let before = manager
            .build_context_for_llm(&plain, Vec::new())
            .expect("context builds");

        let record = crate::domain::conversation::CompactionRecord {
            id: "c".into(),
            conversation_id: plain.id().clone(),
            summary_text: "distilled past".into(),
            up_to_message_id: boundary,
            original_message_count: 4,
            original_tokens: 40,
            summary_tokens: 5,
            compression_ratio: 0.125,
            created_at: chrono::Utc::now(),
        };
        let compacted = ConversationAggregate::from_persistence(
            plain.conversation().clone(),
            plain.messages().to_vec(),
            plain.document_context().to_vec(),
            Some(record),
        );
        let after = manager
            .build_context_for_llm(&compacted, Vec::new())
            .expect("context builds");

        // The folded prefix is gone from the prompt and the summary stands in
        // for it. Before this, QA replayed every message the compaction had
        // already folded, so a compacted conversation grew without bound.
        assert!(
            after.messages.len() < before.messages.len(),
            "compaction must shrink the QA prompt: {} vs {}",
            after.messages.len(),
            before.messages.len()
        );
        assert!(after.messages[0].content.contains("distilled past"));
        assert_ne!(
            after.messages[0].role, "system",
            "a generated summary must not be given system authority here either"
        );
        assert!(
            !after
                .messages
                .iter()
                .any(|message| message.content == "message 0"),
            "a folded message must not also be replayed raw"
        );
    }

    /// An aggregate with `count` alternating messages and a system prompt.
    fn aggregate_with(system_prompt: &str, count: usize) -> ConversationAggregate {
        let mut aggregate = ConversationAggregate::new(
            "QA".to_string(),
            "test-model".to_string(),
            Some(system_prompt.to_string()),
        )
        .expect("aggregate");
        for index in 0..count {
            let role = if index % 2 == 0 {
                crate::domain::conversation::MessageRole::User
            } else {
                crate::domain::conversation::MessageRole::Assistant
            };
            aggregate
                .add_message(role, format!("message {index}"), 10)
                .expect("add message");
        }
        aggregate
    }
}
