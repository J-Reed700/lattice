//! Context Window Builder Service
//!
//! Builds context arrays from conversation history with token-aware truncation.
//!
//! # Architecture
//!
//! The service is split into small, composable operations:
//! - **Self-contained**: All logic for context building is here
//! - **Token-aware**: Respects model token limits (75% for context, 25% for generation)
//! - **Chronological**: Returns messages in oldest-to-newest order
//!
//! # Usage
//!
//! Token-aware truncation:
//! ```rust,no_run
//! use lattice::application::services::context_window_builder::ContextWindowBuilder;
//! use lattice::application::ports::ConversationHistoryPort;
//! use std::sync::Arc;
//!
//! # async fn example(conversation_history: Arc<dyn ConversationHistoryPort>) -> Result<(), Box<dyn std::error::Error>> {
//! let token_counter = Arc::new(|text: &str| text.split_whitespace().count());
//! let builder = ContextWindowBuilder::new(conversation_history, 100_000, token_counter);
//!
//! let context = builder.build("conversation_id").await?;
//! # Ok(())
//! # }
//! ```

use crate::application::ports::ConversationHistoryPort;
use crate::domain::conversation::MessageRole;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;

/// Context window builder for conversation history
///
/// Builds context arrays from conversation messages with token-aware truncation.
/// Reserves 75% of model's max tokens for context, leaving 25% for generation.
pub struct ContextWindowBuilder {
    conversation_history: Arc<dyn ConversationHistoryPort>,
    max_tokens: usize,
    token_counter: Arc<dyn Fn(&str) -> usize + Send + Sync>,
}

impl ContextWindowBuilder {
    /// Create a context window builder.
    ///
    /// Automatically reserves 75% of model's max tokens for context,
    /// leaving 25% for generation.
    ///
    /// # Arguments
    ///
    /// * `conversation_history` - Port for accessing conversation data
    /// * `max_tokens` - Maximum token budget from the model
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::application::services::context_window_builder::ContextWindowBuilder;
    /// # use std::sync::Arc;
    /// # async fn example(conversation_history: Arc<dyn lattice::application::ports::ConversationHistoryPort>) {
    /// let token_counter = Arc::new(|text: &str| text.split_whitespace().count());
    /// let builder = ContextWindowBuilder::new(conversation_history, 100_000, token_counter);
    /// # }
    /// ```
    pub fn new(
        conversation_history: Arc<dyn ConversationHistoryPort>,
        max_tokens: usize,
        token_counter: Arc<dyn Fn(&str) -> usize + Send + Sync>,
    ) -> Self {
        let context_budget = (max_tokens as f32 * 0.75) as usize;
        Self {
            conversation_history,
            max_tokens: context_budget,
            token_counter,
        }
    }

    /// Build context array from conversation history
    ///
    /// The newest completed messages are retained until the model's context
    /// budget is full, then restored to chronological order.
    ///
    /// Returns messages formatted as: ["User: ...", "Assistant: ...", ...]
    /// Ordered chronologically (oldest first).
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - ID of conversation to build context from
    ///
    /// # Returns
    ///
    /// Vector of formatted messages in chronological order
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if database query fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::application::services::context_window_builder::ContextWindowBuilder;
    /// # async fn example(builder: ContextWindowBuilder) -> Result<(), Box<dyn std::error::Error>> {
    /// let context = builder.build("conversation_id").await?;
    /// assert!(!context.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn build(&self, conversation_id: &str) -> Result<Vec<String>> {
        self.build_without_summary(conversation_id).await
    }

    /// Keep the newest completed messages that fit the context budget.
    async fn build_without_summary(&self, conversation_id: &str) -> Result<Vec<String>> {
        let aggregate = self
            .conversation_history
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Conversation not found: {}", conversation_id))
            })?;

        Ok(self.build_from_aggregate(&aggregate))
    }

    /// Assemble a window from an already loaded snapshot without a second read.
    pub fn build_from_aggregate(
        &self,
        aggregate: &crate::domain::conversation::ConversationAggregate,
    ) -> Vec<String> {
        let mut context_rev = Vec::new();
        let mut total_tokens = 0;

        // A compaction summary stands in for the messages it folded, so it is
        // charged to the budget first and the scan below only sees what is still
        // carried raw. Without a compaction this is None and nothing changes.
        // `Assistant:`, never `System:`. A generated summary given system
        // authority sits above the user's own words in precedence, so a
        // paraphrase could revoke a restriction the user actually stated.
        let preamble = aggregate
            .context_preamble()
            .map(|summary| format!("Assistant: {}", summary));
        if let Some(preamble) = &preamble {
            total_tokens += (self.token_counter)(preamble);
        }

        let messages = aggregate.live_messages();

        // Prioritize most recent completed turns, then restore chronological order.
        for msg in messages.iter().rev().filter(|m| m.is_completed()) {
            let formatted = format!(
                "{}: {}",
                match msg.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Assistant",
                    MessageRole::System => "System",
                },
                msg.content
            );

            let msg_tokens = (self.token_counter)(&formatted);

            if total_tokens + msg_tokens > self.max_tokens {
                break;
            }

            context_rev.push(formatted);
            total_tokens += msg_tokens;
        }

        context_rev.reverse();
        match preamble {
            Some(preamble) => std::iter::once(preamble).chain(context_rev).collect(),
            None => context_rev,
        }
    }
}

/// Estimate token count for text
///
/// Uses rough heuristic: ~1.3 tokens per word for English.
/// This is a conservative estimate that works well for most models.
///
/// # Arguments
///
/// * `text` - Text to estimate tokens for
///
/// # Returns
///
/// Estimated token count
///
/// # Example
///
/// ```
/// # use lattice::application::services::context_window_builder::estimate_tokens;
/// assert_eq!(estimate_tokens("hello world"), 2);
/// assert_eq!(estimate_tokens("the quick brown fox"), 5);
/// ```
pub fn estimate_tokens(text: &str) -> usize {
    let words = text.split_whitespace().count();
    (words as f32 * 1.3) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyConversationHistory;

    #[async_trait::async_trait]
    impl ConversationHistoryPort for EmptyConversationHistory {
        async fn get_conversation(
            &self,
            _id: &str,
        ) -> Result<Option<crate::domain::conversation::ConversationAggregate>> {
            Ok(None)
        }
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("hello world"), 2); // 2 words * 1.3 = 2.6 → 2
        assert_eq!(estimate_tokens("the quick brown fox"), 5); // 4 * 1.3 = 5.2 → 5
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("   "), 0);
    }

    #[test]
    fn test_context_budget_calculation() {
        let conversation_history = Arc::new(EmptyConversationHistory);
        let token_counter = Arc::new(|text: &str| estimate_tokens(text));
        let builder = ContextWindowBuilder::new(conversation_history, 100_000, token_counter);

        assert_eq!(builder.max_tokens, 75_000);
    }
}
