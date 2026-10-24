use crate::application::ports::ConversationHistoryPort;
use crate::domain::conversation::MessageRole;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;

/// Context window builder for conversation history.
/// Reserves 75% of model's max tokens for context, leaving 25% for generation.
pub struct ContextWindowBuilder {
    conversation_history: Arc<dyn ConversationHistoryPort>,
    max_tokens: usize,
    token_counter: Arc<dyn Fn(&str) -> usize + Send + Sync>,
}

impl ContextWindowBuilder {
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

    /// Build context array from conversation history in chronological order.
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
            // An answer's pictures replay as their captions: an `img-N` ref
            // belongs to the turn that stored it, and a later answer that
            // copied one would show another turn's picture, or none. A
            // question's attached pictures replay as caption lines too: the
            // pixels rode only in the turn they were sent in.
            let content = match msg.role {
                MessageRole::Assistant => {
                    crate::shared::markdown_media::images_to_captions(&msg.content)
                }
                MessageRole::User => crate::shared::markdown_media::with_picture_captions(
                    &msg.content,
                    &crate::shared::markdown_media::attached_picture_names(msg.metadata.as_deref()),
                ),
                MessageRole::System => msg.content.clone(),
            };
            let formatted = format!(
                "{}: {}",
                match msg.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Assistant",
                    MessageRole::System => "System",
                },
                content
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

/// Estimate token count for text using a rough heuristic (~1.3 tokens per word).
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

    /// An earlier answer's pictures replay as their captions, so a later turn
    /// never sees an `img-N` ref it could copy; what the user wrote is kept.
    #[test]
    fn a_replayed_answer_shows_its_pictures_as_captions() {
        use crate::domain::conversation::{ConversationAggregate, MessageRole};

        let mut aggregate = ConversationAggregate::new("Chat".into(), None).unwrap();
        aggregate
            .add_message(MessageRole::User, "Show me ![it](img-1)".into(), 3)
            .unwrap();
        aggregate
            .add_message(
                MessageRole::Assistant,
                "It flew.\n\n![Saturn V lifting off](img-1)".into(),
                5,
            )
            .unwrap();
        let builder = ContextWindowBuilder::new(
            Arc::new(EmptyConversationHistory),
            100_000,
            Arc::new(|text: &str| estimate_tokens(text)),
        );
        assert_eq!(
            builder.build_from_aggregate(&aggregate),
            [
                "User: Show me ![it](img-1)",
                "Assistant: It flew.\n\n[Picture: Saturn V lifting off]",
            ]
        );
    }

    /// A question's attached pictures replay as caption lines after its
    /// text: the pixels rode only in the turn they were sent in.
    #[test]
    fn a_replayed_question_names_its_attached_pictures() {
        use crate::domain::conversation::{ConversationAggregate, MessageRole};

        let mut aggregate = ConversationAggregate::new("Chat".into(), None).unwrap();
        aggregate
            .add_message(MessageRole::User, "What is in this chart?".into(), 5)
            .unwrap();
        let mut messages = aggregate.messages().to_vec();
        messages[0].metadata =
            Some(r#"{"images":[{"caption":"q3.png","originalName":"Q3 revenue.png"}]}"#.into());
        let aggregate = ConversationAggregate::from_persistence(
            aggregate.conversation().clone(),
            messages,
            Vec::new(),
            None,
        );
        let builder = ContextWindowBuilder::new(
            Arc::new(EmptyConversationHistory),
            100_000,
            Arc::new(|text: &str| estimate_tokens(text)),
        );
        assert_eq!(
            builder.build_from_aggregate(&aggregate),
            ["User: What is in this chart?\n[Picture attached: Q3 revenue.png]"]
        );
    }
}
