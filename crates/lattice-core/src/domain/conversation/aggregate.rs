//! # Conversation Aggregate
//!
//! Rich domain model for LLM conversations following Domain-Driven Design (DDD) patterns.
//!
//! ## Aggregate Root
//!
//! [`ConversationAggregate`] is the aggregate root that encapsulates:
//! - Conversation entity with metadata
//! - Messages (owned entities)
//! - Document references (context)
//!
//! ## Invariants
//!
//! - Conversations must have a title
//! - Messages maintain chronological order
//! - Token counts are tracked accurately
//! - Context window limits are enforced
//!
//! ## Context Management
//!
//! The aggregate manages three types of context:
//! 1. **Conversation History**: User and assistant messages
//! 2. **System Prompt**: Instructions for the LLM
//! 3. **Document Context**: Relevant articles/chunks from RAG

use super::ChatModelChoice;
use crate::shared::error::{AppError, Result};
use crate::shared::types::ConversationId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Conversation aggregate root.
///
/// Encapsulates a conversation with its messages and document context.
/// Enforces business invariants and provides domain operations.
///
/// ## Invariants
///
/// - Conversations must have a non-empty title
/// - Model name must be valid
/// - Messages maintain chronological order
/// - Token counts are accurate
///
/// ## Context Types
///
/// 1. **Conversation History**: Messages exchanged between user and assistant
/// 2. **System Prompt**: Instructions that define assistant behavior
/// 3. **Document Context**: Retrieved chunks from RAG system
#[derive(Debug, Clone)]
pub struct ConversationAggregate {
    conversation: Conversation,
    messages: Vec<ConversationMessage>,
    document_context: Vec<DocumentReference>,
    compaction: Option<CompactionRecord>,
}

impl ConversationAggregate {
    /// Create a new conversation. It follows the Settings default model until
    /// the chat chooses its own.
    ///
    /// # Errors
    ///
    /// Returns an error if the title is empty.
    pub fn new(title: String, system_prompt: Option<String>) -> Result<Self> {
        if title.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Conversation title cannot be empty".into(),
            ));
        }

        Ok(Self {
            conversation: Conversation {
                id: ConversationId::new(),
                title,
                model_choice: ChatModelChoice::Default,
                system_prompt,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                message_count: 0,
                total_tokens: 0,
            },
            messages: Vec::new(),
            document_context: Vec::new(),
            compaction: None,
        })
    }

    /// Reconstruct a conversation aggregate from persistence.
    ///
    /// This method is used by the repository layer to reconstruct aggregates from the database.
    /// Unlike `new()`, this does not validate since data from the database is already trusted.
    pub fn from_persistence(
        conversation: Conversation,
        messages: Vec<ConversationMessage>,
        document_context: Vec<DocumentReference>,
        compaction: Option<CompactionRecord>,
    ) -> Self {
        Self {
            conversation,
            messages,
            document_context,
            compaction,
        }
    }

    /// Add a message to the conversation.
    ///
    /// # Errors
    ///
    /// Returns an error if content is empty.
    ///
    /// # Invariants
    ///
    /// - Messages are appended in chronological order
    /// - Message count and token totals are updated
    pub fn add_message(&mut self, role: MessageRole, content: String, tokens: i64) -> Result<()> {
        self.add_message_with_status(role, content, tokens, "completed".to_string())?;
        Ok(())
    }

    /// Add a message with a specific status (for two-phase commit).
    ///
    /// # Errors
    ///
    /// Returns an error if content is empty.
    ///
    /// # Invariants
    ///
    /// - Messages are appended in chronological order
    /// - Message count and token totals are updated
    pub fn add_message_with_status(
        &mut self,
        role: MessageRole,
        content: String,
        tokens: i64,
        status: String,
    ) -> Result<String> {
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Message content cannot be empty".into(),
            ));
        }

        let message_id = uuid::Uuid::new_v4().to_string();
        let message = ConversationMessage {
            id: message_id.clone(),
            conversation_id: self.conversation.id.clone(),
            role,
            content,
            tokens,
            created_at: Utc::now(),
            metadata: None,
            status,
        };

        self.messages.push(message);
        self.conversation.message_count += 1;
        self.conversation.total_tokens += tokens;
        self.conversation.updated_at = Utc::now();

        Ok(message_id)
    }

    /// Update the status of a message.
    ///
    /// Used for two-phase commit pattern to update message status after operations complete.
    ///
    /// # Errors
    ///
    /// Returns an error if message is not found.
    pub fn update_message_status(&mut self, message_id: &str, status: String) -> Result<()> {
        let message = self
            .messages
            .iter_mut()
            .find(|m| m.id == message_id)
            .ok_or_else(|| AppError::NotFound(format!("Message {} not found", message_id)))?;

        message.status = status;
        self.conversation.updated_at = Utc::now();

        Ok(())
    }

    /// Add document reference to the conversation context.
    ///
    /// This tracks which documents are relevant to this conversation.
    pub fn add_document_reference(
        &mut self,
        document_id: String,
        chunk_id: Option<String>,
        relevance_score: Option<f32>,
    ) {
        let reference = DocumentReference {
            document_id,
            chunk_id,
            relevance_score,
            added_at: Utc::now(),
        };

        // Avoid duplicates
        if !self
            .document_context
            .iter()
            .any(|r| r.document_id == reference.document_id && r.chunk_id == reference.chunk_id)
        {
            self.document_context.push(reference);
        }
    }

    /// Prune messages to fit within a token limit.
    ///
    /// Removes oldest messages (keeping most recent) until total tokens <= max_tokens.
    /// Always preserves at least the last message to maintain context.
    ///
    /// # Errors
    ///
    /// Returns an error if max_tokens is zero or if the last message alone exceeds the limit.
    ///
    /// # Invariants
    ///
    /// - At least one message is retained (if any exist)
    /// - Most recent messages are preserved
    /// - Token count is accurate after pruning
    pub fn prune_to_token_limit(&mut self, max_tokens: i64) -> Result<()> {
        if max_tokens <= 0 {
            return Err(AppError::InvalidInput(
                "Token limit must be positive".into(),
            ));
        }

        if self.messages.is_empty() {
            return Ok(());
        }

        let mut cumulative_tokens = 0;
        let mut keep_from_index = self.messages.len();

        for (i, message) in self.messages.iter().enumerate().rev() {
            cumulative_tokens += message.tokens;
            if cumulative_tokens > max_tokens {
                keep_from_index = i + 1;
                break;
            }
        }

        // If keep_from_index is still at messages.len(), all messages fit
        if keep_from_index == self.messages.len() {
            return Ok(());
        }

        // Otherwise, ensure we keep at least the last message
        keep_from_index = keep_from_index.min(self.messages.len() - 1);

        if keep_from_index > 0 {
            self.messages.drain(0..keep_from_index);

            // Recalculate totals
            self.conversation.message_count = self.messages.len() as i64;
            self.conversation.total_tokens = self.messages.iter().map(|m| m.tokens).sum();
            self.conversation.updated_at = Utc::now();
        }

        Ok(())
    }

    /// The summary standing in for the folded prefix, if a compaction is active.
    ///
    /// Every context window is assembled the same way: this preamble first, then
    /// [`Self::live_messages`]. Keeping both halves of that decision here is what
    /// stops one caller from carrying the summary and another the raw history.
    pub fn context_preamble(&self) -> Option<String> {
        self.compaction.as_ref().map(|record| {
            // The shared framing, so every path says the summary is a model's
            // paraphrase, not the transcript.
            crate::domain::conversation::memory::frame_generated_summary(&record.summary_text)
        })
    }

    /// The messages a context window still carries verbatim.
    ///
    /// With a compaction active that is everything after its boundary; without
    /// one, every message. If the boundary message is no longer present — it was
    /// deleted, or truncated away — this returns *all* messages rather than none:
    /// a stale boundary may cost tokens, but it must never silently erase the
    /// live conversation.
    pub fn live_messages(&self) -> &[ConversationMessage] {
        let Some(record) = &self.compaction else {
            return &self.messages;
        };
        self.messages
            .iter()
            .position(|m| m.id == record.up_to_message_id)
            .and_then(|boundary| self.messages.get(boundary + 1..))
            .unwrap_or(&self.messages)
    }

    /// Rename the conversation.
    pub fn rename(&mut self, new_title: String) -> Result<()> {
        if new_title.trim().is_empty() {
            return Err(AppError::InvalidInput("Title cannot be empty".into()));
        }
        self.conversation.title = new_title;
        self.conversation.updated_at = Utc::now();
        Ok(())
    }

    /// Update the system prompt.
    pub fn update_system_prompt(&mut self, system_prompt: Option<String>) {
        self.conversation.system_prompt = system_prompt;
        self.conversation.updated_at = Utc::now();
    }

    // Getters

    pub fn id(&self) -> &ConversationId {
        &self.conversation.id
    }

    pub fn title(&self) -> &str {
        &self.conversation.title
    }

    pub fn model_choice(&self) -> &ChatModelChoice {
        &self.conversation.model_choice
    }

    pub fn system_prompt(&self) -> Option<&str> {
        self.conversation.system_prompt.as_deref()
    }

    pub fn messages(&self) -> &[ConversationMessage] {
        &self.messages
    }

    pub fn document_context(&self) -> &[DocumentReference] {
        &self.document_context
    }

    /// The active compaction, if any.
    pub fn compaction(&self) -> Option<&CompactionRecord> {
        self.compaction.as_ref()
    }

    pub fn message_count(&self) -> i64 {
        self.conversation.message_count
    }

    pub fn total_tokens(&self) -> i64 {
        self.conversation.total_tokens
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.conversation.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.conversation.updated_at
    }

    /// Get the conversation entity (for persistence).
    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }
}

/// Conversation entity.
///
/// Core conversation metadata without messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: ConversationId,
    pub title: String,
    /// The model this chat asked for; `Default` follows Settings.
    pub model_choice: ChatModelChoice,
    pub system_prompt: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: i64,
    pub total_tokens: i64,
}

/// Conversation message.
///
/// Represents a single message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    pub conversation_id: ConversationId,
    pub role: MessageRole,
    pub content: String,
    pub tokens: i64,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<String>, // JSON field for additional data
    pub status: String,           // Message status: 'pending', 'completed', 'failed'
}

/// Canonical conversation message statuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationMessageStatus {
    Pending,
    Completed,
    Failed,
    Unknown,
}

impl ConversationMessageStatus {
    pub const PENDING: &'static str = "pending";
    pub const COMPLETED: &'static str = "completed";
    pub const FAILED: &'static str = "failed";

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => Self::PENDING,
            Self::Completed => Self::COMPLETED,
            Self::Failed => Self::FAILED,
            Self::Unknown => "",
        }
    }
}

impl std::str::FromStr for ConversationMessageStatus {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        let normalized = s.trim().to_ascii_lowercase();
        match normalized.as_str() {
            Self::PENDING => Ok(Self::Pending),
            Self::COMPLETED => Ok(Self::Completed),
            Self::FAILED => Ok(Self::Failed),
            _ => Ok(Self::Unknown),
        }
    }
}

impl ConversationMessage {
    pub fn status_kind(&self) -> ConversationMessageStatus {
        self.status
            .parse::<ConversationMessageStatus>()
            .unwrap_or(ConversationMessageStatus::Unknown)
    }

    pub fn is_completed(&self) -> bool {
        matches!(self.status_kind(), ConversationMessageStatus::Completed)
    }

    pub fn is_pending(&self) -> bool {
        matches!(self.status_kind(), ConversationMessageStatus::Pending)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.status_kind(), ConversationMessageStatus::Failed)
    }
}

/// Message role in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
            MessageRole::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for MessageRole {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "system" => Ok(MessageRole::System),
            _ => Err(AppError::InvalidInput(format!(
                "Invalid message role: {}",
                s
            ))),
        }
    }
}

/// Reference to a document in the conversation context.
///
/// Tracks which documents/chunks are relevant to this conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentReference {
    pub document_id: String,
    pub chunk_id: Option<String>,
    pub relevance_score: Option<f32>,
    pub added_at: DateTime<Utc>,
}

/// A compaction of the oldest messages in a conversation.
///
/// When context grows too large, the oldest messages are folded into a summary.
/// The original messages are retained for display, but the LLM context window
/// carries this summary plus only the messages after `up_to_message_id`.
/// Backed by the `conversation_summaries` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionRecord {
    /// Stable identifier for this compaction.
    pub id: String,
    /// The conversation this compaction belongs to.
    pub conversation_id: ConversationId,
    /// The LLM-produced summary of the compacted messages.
    pub summary_text: String,
    /// Id of the last message folded into the summary (inclusive boundary).
    pub up_to_message_id: String,
    /// How many messages were folded into the summary.
    pub original_message_count: i64,
    /// Total tokens of the folded messages before summarization.
    pub original_tokens: i64,
    /// Token count of the summary itself.
    pub summary_tokens: i64,
    /// `summary_tokens / original_tokens`, clamped to `(0.0, 1.0]`.
    pub compression_ratio: f64,
    /// When the compaction was created.
    pub created_at: DateTime<Utc>,
}

/// Message format for LLM API calls.
///
/// Simplified format compatible with Anthropic Messages API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMMessage {
    pub role: String,
    pub content: String,
}

#[cfg(test)]
mod tests;
