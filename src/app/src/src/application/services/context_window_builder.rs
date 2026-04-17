//! Context Window Builder Service
//!
//! Builds context arrays from conversation history with token-aware truncation
//! and optional summarization for long conversations.
//!
//! # Architecture
//!
//! This service follows the "bricks and studs" philosophy:
//! - **Self-contained**: All logic for context building is here
//! - **Token-aware**: Respects model token limits (75% for context, 25% for generation)
//! - **Chronological**: Returns messages in oldest-to-newest order
//! - **Summarization**: Optional compression of old messages to fit more context
//!
//! # Usage
//!
//! Without summarization (simple truncation):
//! ```rust,no_run
//! use vault_desktop::application::services::context_window_builder::ContextWindowBuilder;
//! use vault_desktop::infrastructure::services::ConversationService;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let conversation_service = Arc::new(ConversationService::new(pool));
//! let token_counter = Arc::new(|text: &str| text.split_whitespace().count());
//! let builder = ContextWindowBuilder::new(conversation_service, 100_000, token_counter);
//!
//! let context = builder.build("conversation_id").await?;
//! # Ok(())
//! # }
//! ```
//!
//! With summarization (compress old messages):
//! ```rust,no_run
//! # use vault_desktop::application::services::context_window_builder::ContextWindowBuilder;
//! # use vault_desktop::application::services::conversation_summarizer::ConversationSummarizer;
//! # use std::sync::Arc;
//! # async fn example(conversation_service: Arc<ConversationService>, summarizer: Arc<ConversationSummarizer>) -> Result<(), Box<dyn std::error::Error>> {
//! let token_counter = Arc::new(|text: &str| text.split_whitespace().count());
//! let builder = ContextWindowBuilder::new(conversation_service, 100_000, token_counter)
//!     .with_summarization(summarizer)
//!     .with_recent_message_count(15); // Keep last 15 messages verbatim
//!
//! let context = builder.build("conversation_id").await?;
//! # Ok(())
//! # }
//! ```

use crate::application::services::conversation_summarizer::ConversationSummarizer;
use crate::domain::conversation::MessageRole;
use crate::infrastructure::services::ConversationService;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;

/// Context window builder for conversation history
///
/// Builds context arrays from conversation messages with token-aware truncation
/// and optional summarization for long conversations.
/// Reserves 75% of model's max tokens for context, leaving 25% for generation.
pub struct ContextWindowBuilder {
    conversation_service: Arc<ConversationService>,
    max_tokens: usize,
    token_counter: Arc<dyn Fn(&str) -> usize + Send + Sync>,
    /// Optional summarizer for context compression
    summarizer: Option<Arc<ConversationSummarizer>>,
    /// Number of recent messages to keep verbatim (default: 10)
    recent_message_count: usize,
    /// Soft cap percentage - trigger summarization (default: 70%)
    soft_cap_percent: f32,
    /// Hard cap percentage - aggressive summarization (default: 85%)
    hard_cap_percent: f32,
}

impl ContextWindowBuilder {
    /// Create new context window builder with optional summarization
    ///
    /// Automatically reserves 75% of model's max tokens for context,
    /// leaving 25% for generation.
    ///
    /// # Arguments
    ///
    /// * `conversation_service` - Service for accessing conversation data
    /// * `max_tokens` - Maximum token budget from the model
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::services::context_window_builder::ContextWindowBuilder;
    /// # use std::sync::Arc;
    /// # async fn example(conversation_service: Arc<ConversationService>) {
    /// let token_counter = Arc::new(|text: &str| text.split_whitespace().count());
    /// let builder = ContextWindowBuilder::new(conversation_service, 100_000, token_counter);
    /// # }
    /// ```
    pub fn new(
        conversation_service: Arc<ConversationService>,
        max_tokens: usize,
        token_counter: Arc<dyn Fn(&str) -> usize + Send + Sync>,
    ) -> Self {
        let context_budget = (max_tokens as f32 * 0.75) as usize;
        Self {
            conversation_service,
            max_tokens: context_budget,
            token_counter,
            summarizer: None,         // Disabled by default
            recent_message_count: 10, // Keep last 10 messages verbatim
            soft_cap_percent: 0.70,
            hard_cap_percent: 0.85,
        }
    }

    /// Enable summarization with provided summarizer
    ///
    /// When enabled, old messages will be summarized when approaching token limit,
    /// while recent messages are kept verbatim.
    ///
    /// # Arguments
    ///
    /// * `summarizer` - Conversation summarizer instance
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::services::context_window_builder::ContextWindowBuilder;
    /// # use vault_desktop::application::services::conversation_summarizer::ConversationSummarizer;
    /// # use std::sync::Arc;
    /// # async fn example(conversation_service: Arc<ConversationService>, summarizer: Arc<ConversationSummarizer>) {
    /// let token_counter = Arc::new(|text: &str| text.split_whitespace().count());
    /// let builder = ContextWindowBuilder::new(conversation_service, 100_000, token_counter)
    ///     .with_summarization(summarizer);
    /// # }
    /// ```
    pub fn with_summarization(mut self, summarizer: Arc<ConversationSummarizer>) -> Self {
        self.summarizer = Some(summarizer);
        self
    }

    /// Set number of recent messages to keep verbatim (default: 10)
    ///
    /// When summarization is enabled, this many recent messages will be kept
    /// verbatim while older messages are summarized.
    ///
    /// # Arguments
    ///
    /// * `count` - Number of recent messages to preserve
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::services::context_window_builder::ContextWindowBuilder;
    /// # use std::sync::Arc;
    /// # async fn example(conversation_service: Arc<ConversationService>) {
    /// let token_counter = Arc::new(|text: &str| text.split_whitespace().count());
    /// let builder = ContextWindowBuilder::new(conversation_service, 100_000, token_counter)
    ///     .with_recent_message_count(15);
    /// # }
    /// ```
    pub fn with_recent_message_count(mut self, count: usize) -> Self {
        self.recent_message_count = count;
        self
    }

    /// Build context array from conversation history
    ///
    /// With summarization enabled:
    /// - Recent messages (last N) kept verbatim
    /// - Older messages summarized when approaching token limit
    /// - Falls back to truncation if summarization fails
    ///
    /// Without summarization:
    /// - Simple truncation when budget exceeded (current behavior)
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
    /// # use vault_desktop::application::services::context_window_builder::ContextWindowBuilder;
    /// # async fn example(builder: ContextWindowBuilder) -> Result<(), Box<dyn std::error::Error>> {
    /// let context = builder.build("conversation_id").await?;
    /// assert!(!context.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn build(&self, conversation_id: &str) -> Result<Vec<String>> {
        // Try summarization if enabled
        if let Some(summarizer) = &self.summarizer {
            match self.build_with_summary(conversation_id, summarizer).await {
                Ok(context) => {
                    tracing::debug!(
                        conversation_id,
                        context_items = context.len(),
                        "Context built with summarization"
                    );
                    return Ok(context);
                }
                Err(e) => {
                    tracing::warn!(
                        conversation_id,
                        error = %e,
                        "Summarization failed, falling back to truncation"
                    );
                    // Fall through to truncation
                }
            }
        }

        // Fallback: use simple truncation (original behavior)
        self.build_without_summary(conversation_id).await
    }

    /// Build context with summarization
    ///
    /// Strategy:
    /// 1. Split messages into old (to summarize) and recent (verbatim)
    /// 2. Check if we need summarization (based on token budget)
    /// 3. If needed, get or generate summary for old messages
    /// 4. Combine: [summary] + [recent messages]
    async fn build_with_summary(
        &self,
        conversation_id: &str,
        summarizer: &Arc<ConversationSummarizer>,
    ) -> Result<Vec<String>> {
        // Load conversation
        let aggregate = self
            .conversation_service
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Conversation not found: {}", conversation_id))
            })?;

        let messages = aggregate.messages();

        // Only include completed turns in model context.
        // Pending/streaming/error messages represent in-flight or failed operations.
        let active_messages: Vec<_> = messages.iter().filter(|m| m.is_completed()).collect();

        if active_messages.is_empty() {
            return Ok(Vec::new());
        }

        // Calculate total tokens for all messages
        let total_tokens: usize = active_messages
            .iter()
            .map(|m| (self.token_counter)(&format!("{}: {}", m.role, m.content)))
            .sum();

        let soft_cap = (self.max_tokens as f32 * self.soft_cap_percent) as usize;

        // If under soft cap AND still within the recent-message window, no summary is needed.
        // Once conversation grows beyond recent_message_count, prefer summary + recent pattern.
        if total_tokens <= soft_cap && active_messages.len() <= self.recent_message_count {
            tracing::debug!(
                conversation_id,
                total_tokens,
                soft_cap,
                active_messages = active_messages.len(),
                recent_message_count = self.recent_message_count,
                "Under soft cap with short history, no summarization needed"
            );
            return self.build_without_summary(conversation_id).await;
        }

        // Split into old and recent messages
        let message_count = active_messages.len();
        let split_point = message_count.saturating_sub(self.recent_message_count);

        let old_messages: Vec<_> = active_messages
            .get(..split_point)
            .unwrap_or(&[])
            .iter()
            .map(|&m| m.clone())
            .collect();
        let recent_messages = active_messages.get(split_point..).unwrap_or(&[]);

        // If no old messages to summarize, use truncation
        if old_messages.is_empty() {
            tracing::debug!(conversation_id, "All messages are recent, using truncation");
            return self.build_without_summary(conversation_id).await;
        }

        // Calculate tokens for old messages
        let old_tokens: usize = old_messages
            .iter()
            .map(|m| (self.token_counter)(&format!("{}: {}", m.role, m.content)))
            .sum();

        // Prefer cached summary for low-latency chat path.
        // If not cached/valid, schedule background refresh and continue with recent turns.
        let summary = match old_messages.last() {
            Some(last_old_message) => {
                let cached = summarizer
                    .load_valid_summary(conversation_id, &last_old_message.id)
                    .await?;

                if cached.is_none() {
                    summarizer.spawn_refresh_summary(
                        conversation_id.to_string(),
                        old_messages.clone(),
                        old_tokens,
                    );
                }

                cached
            }
            None => None,
        };

        // Build context: [summary (if available)] + [recent messages]
        let mut context = Vec::new();
        let mut total_tokens = 0;

        // Add summary first when available.
        if let Some(summary) = summary {
            tracing::info!(
                conversation_id,
                old_message_count = old_messages.len(),
                recent_message_count = recent_messages.len(),
                compression_ratio = summary.compression_ratio,
                "Using summarized context"
            );

            let summary_formatted =
                format!("System: CONVERSATION SUMMARY: {}", summary.summary_text);
            let summary_tokens = (self.token_counter)(&summary_formatted);

            if total_tokens + summary_tokens <= self.max_tokens {
                context.push(summary_formatted);
                total_tokens += summary_tokens;
            }
        } else {
            tracing::debug!(
                conversation_id,
                old_message_count = old_messages.len(),
                recent_message_count = recent_messages.len(),
                "No cached summary yet, using recent messages while background summary builds"
            );
        }

        // Add recent messages verbatim, prioritizing newest turns when near the token cap.
        // We collect in reverse then restore chronological order for the final context.
        let mut recent_formatted_rev: Vec<String> = Vec::new();
        for msg in recent_messages.iter().rev() {
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

            // Stop if adding this message exceeds budget
            if total_tokens + msg_tokens > self.max_tokens {
                tracing::warn!(
                    conversation_id,
                    "Recent messages exceed budget, truncating oldest turns"
                );
                break;
            }

            recent_formatted_rev.push(formatted);
            total_tokens += msg_tokens;
        }

        recent_formatted_rev.reverse();
        context.extend(recent_formatted_rev);

        Ok(context)
    }

    /// Build context without summarization (original behavior)
    ///
    /// Simple truncation: keeps messages until token budget exceeded
    async fn build_without_summary(&self, conversation_id: &str) -> Result<Vec<String>> {
        // Original implementation from current build() method
        let aggregate = self
            .conversation_service
            .get_conversation(conversation_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Conversation not found: {}", conversation_id))
            })?;

        let mut context_rev = Vec::new();
        let mut total_tokens = 0;

        let messages = aggregate.messages();

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
        let context = context_rev;

        Ok(context)
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
/// # use vault_desktop::application::services::context_window_builder::estimate_tokens;
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

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("hello world"), 2); // 2 words * 1.3 = 2.6 → 2
        assert_eq!(estimate_tokens("the quick brown fox"), 5); // 4 * 1.3 = 5.2 → 5
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("   "), 0);
    }

    #[tokio::test]
    async fn test_context_budget_calculation() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let mock_service = Arc::new(ConversationService::new(pool));
        let token_counter = Arc::new(|text: &str| estimate_tokens(text));
        let builder = ContextWindowBuilder::new(mock_service, 100_000, token_counter);

        assert_eq!(builder.max_tokens, 75_000);
        assert_eq!(builder.recent_message_count, 10);
        assert_eq!(builder.soft_cap_percent, 0.70);
        assert_eq!(builder.hard_cap_percent, 0.85);
        assert!(builder.summarizer.is_none());
    }
}
