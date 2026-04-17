use crate::application::ports::LLMPort;
use crate::domain::conversation::ConversationMessage;
use std::sync::Arc;

/// Conversation-scoped background processing events.
#[derive(Clone)]
pub enum ConversationEvent {
    SummaryRefreshRequested(SummaryRefreshRequestedEvent),
}

impl ConversationEvent {
    pub fn summary_refresh_requested(
        conversation_id: String,
        completed_messages: Vec<ConversationMessage>,
        original_tokens: usize,
        llm: Arc<dyn LLMPort>,
    ) -> Self {
        Self::SummaryRefreshRequested(SummaryRefreshRequestedEvent {
            conversation_id,
            completed_messages,
            original_tokens,
            llm,
        })
    }
}

/// Request to generate/persist a refreshed conversation summary.
#[derive(Clone)]
pub struct SummaryRefreshRequestedEvent {
    pub conversation_id: String,
    pub completed_messages: Vec<ConversationMessage>,
    pub original_tokens: usize,
    pub llm: Arc<dyn LLMPort>,
}
