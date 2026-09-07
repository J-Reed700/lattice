//! Read-only access to conversation history.
//!
//! Application services use this port instead of depending on a concrete
//! persistence-backed conversation service.

use crate::domain::conversation::ConversationAggregate;
use crate::shared::error::Result;
use async_trait::async_trait;

/// Provides the conversation data needed to assemble an LLM context window.
#[async_trait]
pub trait ConversationHistoryPort: Send + Sync {
    /// Load a conversation aggregate, including its messages.
    async fn get_conversation(&self, id: &str) -> Result<Option<ConversationAggregate>>;
}
