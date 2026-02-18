//! Conversation Summarizer Service
//!
//! Generates and stores conversation summaries through the active LLM backend.

use crate::application::ports::LLMPort;
use crate::domain::conversation::ConversationMessage;
use crate::domain::conversation_summary::ConversationSummary;
use crate::infrastructure::persistence::repositories::summary_repository::SummaryRepository;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;

/// Summarizer service for conversation history
#[derive(Clone)]
pub struct ConversationSummarizer {
    llm: Arc<dyn LLMPort>,
    repository: Arc<SummaryRepository>,
}

impl ConversationSummarizer {
    /// Create new summarizer
    pub fn new(llm: Arc<dyn LLMPort>, repository: Arc<SummaryRepository>) -> Self {
        Self { llm, repository }
    }

    /// Summarize messages using LLM
    ///
    /// Generates a concise summary preserving key information
    async fn summarize_messages(&self, messages: &[ConversationMessage]) -> Result<String> {
        if messages.is_empty() {
            return Err(AppError::InvalidInput(
                "Cannot summarize empty messages".to_string(),
            ));
        }

        // Format messages for summarization
        let conversation_text = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        // Summarization instruction goes into the context as a system message so all LLM
        // backends can consume it through the common LLMPort interface.
        let system_context = vec![
            "System: You summarize conversations concisely. Preserve key decisions, intent, constraints, and unresolved items. Use 2-4 short paragraphs.".to_string(),
        ];

        // User prompt with bounded transcript.
        let prompt = format!(
            "Summarize this conversation in 2-4 paragraphs. Focus on durable memory and omit filler:\n\n{}",
            conversation_text
        );

        // Generate summary
        let summary = self
            .llm
            .generate(&prompt, &system_context, None)
            .await
            .map_err(|e| AppError::Other(format!("Summarization failed: {}", e)))?;

        Ok(summary.trim().to_string())
    }

    /// Generate and persist a fresh summary for the conversation.
    pub async fn refresh_summary(
        &self,
        conversation_id: &str,
        messages: &[ConversationMessage],
        original_tokens: usize,
    ) -> Result<ConversationSummary> {
        // Defensive check: ensure messages is not empty
        // Even though we check above, use ok_or_else for defense-in-depth
        let last_message_id = messages
            .last()
            .ok_or_else(|| {
                AppError::InvalidInput("Cannot summarize empty conversation".to_string())
            })?
            .id
            .clone();

        tracing::info!(
            conversation_id,
            message_count = messages.len(),
            "Generating fresh conversation summary"
        );

        let summary_text = self.summarize_messages(messages).await?;
        let summary_tokens = estimate_tokens(&summary_text);

        // Create summary value object
        let summary = ConversationSummary::new(
            conversation_id.to_string(),
            summary_text,
            last_message_id,
            messages.len(),
            original_tokens,
            summary_tokens,
        )
        .map_err(AppError::InvalidInput)?;

        // Persist summary (upsert)
        self.repository.save(&summary).await?;

        tracing::info!(
            conversation_id,
            compression_ratio = summary.compression_ratio,
            original_tokens,
            summary_tokens,
            "Summary generated and stored"
        );

        Ok(summary)
    }

    /// Backward-compatible wrapper. Always performs a fresh summary refresh.
    pub async fn get_or_create_summary(
        &self,
        conversation_id: &str,
        messages: &[ConversationMessage],
        original_tokens: usize,
    ) -> Result<ConversationSummary> {
        self.refresh_summary(conversation_id, messages, original_tokens)
            .await
    }

    /// Invalidate cached summary
    pub async fn invalidate_summary(&self, conversation_id: &str) -> Result<()> {
        self.repository.delete(conversation_id).await
    }

    /// Load summary only if it is valid for the current message boundary.
    pub async fn load_valid_summary(
        &self,
        conversation_id: &str,
        last_message_id: &str,
    ) -> Result<Option<ConversationSummary>> {
        let cached = self.repository.load(conversation_id).await?;
        Ok(cached.filter(|summary| summary.is_valid_for_messages(last_message_id)))
    }

    /// Fire-and-forget summary refresh to avoid blocking chat response latency.
    pub fn spawn_refresh_summary(
        &self,
        conversation_id: String,
        messages: Vec<ConversationMessage>,
        original_tokens: usize,
    ) {
        let summarizer = self.clone();
        tokio::spawn(async move {
            if let Err(e) = summarizer
                .refresh_summary(&conversation_id, &messages, original_tokens)
                .await
            {
                tracing::warn!(
                    conversation_id,
                    error = %e,
                    "Background conversation summary refresh failed"
                );
            }
        });
    }
}

/// Estimate token count (reuse from context_window_builder)
fn estimate_tokens(text: &str) -> usize {
    let words = text.split_whitespace().count();
    (words as f32 * 1.3) as usize
}
