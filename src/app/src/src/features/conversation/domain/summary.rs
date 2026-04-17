//! Conversation Summary Domain Model
//!
//! Value object representing a summarized conversation history.
//! Enforces invariants: valid compression ratio, non-empty summary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Conversation summary value object
///
/// Represents a compressed version of conversation history for efficient
/// context window management. Summaries are cached and invalidated when
/// new messages are added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub conversation_id: String,
    pub summary_text: String,
    pub up_to_message_id: String,
    pub original_message_count: usize,
    pub original_tokens: usize,
    pub summary_tokens: usize,
    pub compression_ratio: f32,
    pub created_at: DateTime<Utc>,
}

impl ConversationSummary {
    /// Create new conversation summary with validation
    ///
    /// # Invariants
    /// - summary_text must be non-empty
    /// - compression_ratio must be > 0 and <= 1.0
    /// - summary_tokens must be less than original_tokens
    pub fn new(
        conversation_id: String,
        summary_text: String,
        up_to_message_id: String,
        original_message_count: usize,
        original_tokens: usize,
        summary_tokens: usize,
    ) -> Result<Self, String> {
        // Validate non-empty summary
        if summary_text.trim().is_empty() {
            return Err("Summary text cannot be empty".to_string());
        }

        // Validate token counts
        if summary_tokens >= original_tokens {
            return Err(format!(
                "Summary ({} tokens) must be smaller than original ({} tokens)",
                summary_tokens, original_tokens
            ));
        }

        // Calculate compression ratio
        let compression_ratio = summary_tokens as f32 / original_tokens as f32;

        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id,
            summary_text,
            up_to_message_id,
            original_message_count,
            original_tokens,
            summary_tokens,
            compression_ratio,
            created_at: Utc::now(),
        })
    }

    /// Check if summary is valid for given messages
    ///
    /// Returns false if last_message_id has changed (invalidates cache)
    pub fn is_valid_for_messages(&self, last_message_id: &str) -> bool {
        self.up_to_message_id == last_message_id
    }

    /// Check if summary achieved effective compression
    ///
    /// Returns true if compression ratio <= 0.5 (50% or better)
    pub fn is_effective(&self) -> bool {
        self.compression_ratio <= 0.5
    }
}
