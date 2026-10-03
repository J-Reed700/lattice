//! Supplemental persisted context used when assembling a chat prompt.

use crate::shared::error::Result;

#[derive(Debug, Clone)]
pub struct LinkedConversationSource {
    pub title: Option<String>,
    pub url: String,
    pub excerpt: Option<String>,
}

#[async_trait::async_trait]
pub trait ConversationContextPort: Send + Sync {
    /// The prompt the conversation's surroundings give it: an Explorer
    /// thread's folder instructions when the folder has some, otherwise its
    /// space's prompt. A conversation's own prompt still wins over either.
    async fn space_prompt(&self, conversation_id: &str) -> Result<Option<String>>;
    /// Return the most recently added sources first, up to the requested limit.
    async fn linked_sources(
        &self,
        conversation_id: &str,
        limit: i64,
    ) -> Result<Vec<LinkedConversationSource>>;
}
