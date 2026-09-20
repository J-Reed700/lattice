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
//! - Conversations must have a valid model name
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
//!
//! ## Usage
//!
//! ```rust,no_run
//! use lattice::domain::{ConversationAggregate, MessageRole};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut conversation = ConversationAggregate::new(
//!     "My Conversation".to_string(),
//!     "claude-sonnet-4-5-20250929".to_string(),
//!     Some("You are a helpful assistant.".to_string()),
//! );
//!
//! // Add user message
//! conversation.add_message(
//!     MessageRole::User,
//!     "What is RAG?".to_string(),
//!     50, // token count
//! )?;
//!
//! // Add assistant response
//! conversation.add_message(
//!     MessageRole::Assistant,
//!     "RAG stands for...".to_string(),
//!     200,
//! )?;
//!
//! // Check if we need to prune due to token limits
//! if conversation.total_tokens() > 10000 {
//!     conversation.prune_to_token_limit(8000)?;
//! }
//! # Ok(())
//! # }
//! ```

use crate::shared::domain_types::ConversationId;
use crate::shared::error::{AppError, Result};
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
    /// Create a new conversation.
    ///
    /// # Arguments
    ///
    /// * `title` - Human-readable conversation title
    /// * `model_name` - LLM model identifier (e.g., "claude-sonnet-4-5-20250929")
    /// * `system_prompt` - Optional system instructions for the LLM
    ///
    /// # Errors
    ///
    /// Returns an error if title or model_name is empty.
    ///
    /// # Invariants
    ///
    /// - Title is non-empty
    /// - Model name is non-empty
    pub fn new(title: String, model_name: String, system_prompt: Option<String>) -> Result<Self> {
        if title.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Conversation title cannot be empty".into(),
            ));
        }
        if model_name.trim().is_empty() {
            return Err(AppError::InvalidInput("Model name cannot be empty".into()));
        }

        Ok(Self {
            conversation: Conversation {
                id: ConversationId::new(),
                title,
                model_name,
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
    ///
    /// # Arguments
    ///
    /// * `conversation` - The conversation entity from the database
    /// * `messages` - All messages for this conversation
    /// * `document_context` - All document references for this conversation
    ///
    /// # Usage
    ///
    /// This is primarily for repository use:
    /// ```rust,no_run
    /// # use lattice::domain::{Conversation, ConversationMessage, DocumentReference, ConversationAggregate};
    /// # fn example(conversation: Conversation, messages: Vec<ConversationMessage>, refs: Vec<DocumentReference>) {
    /// let aggregate = ConversationAggregate::from_persistence(conversation, messages, refs, None);
    /// # }
    /// ```
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
    /// # Arguments
    ///
    /// * `role` - Message role (User, Assistant, System)
    /// * `content` - Message content
    /// * `tokens` - Token count
    /// * `status` - Message status ('pending', 'completed', 'failed')
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
    /// # Arguments
    ///
    /// * `message_id` - ID of the message to update
    /// * `status` - New status ('pending', 'completed', 'failed')
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
            // Shared framing, not a local format string: this used to say only
            // "summarized", which let the QA and context-window paths present a
            // model's paraphrase as though it were the transcript.
            crate::domain::conversation_memory::frame_generated_summary(&record.summary_text)
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

    pub fn model_name(&self) -> &str {
        &self.conversation.model_name
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
    pub model_name: String,
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
mod tests {
    use super::*;

    /// An aggregate with `count` alternating user/assistant messages, 10 tokens each.
    fn aggregate_with_messages(count: usize) -> ConversationAggregate {
        let mut aggregate =
            ConversationAggregate::new("Test".to_string(), "test-model".to_string(), None)
                .expect("aggregate");
        for i in 0..count {
            let role = if i % 2 == 0 {
                MessageRole::User
            } else {
                MessageRole::Assistant
            };
            aggregate
                .add_message(role, format!("message {}", i), 10)
                .expect("add message");
        }
        aggregate
    }

    /// Attach a compaction the way persistence does: a `CompactionRecord` read
    /// back from `conversation_summaries`.
    ///
    /// There is deliberately no aggregate method that creates one. A summary row
    /// is written in exactly one place — the memory commit — so that the summary
    /// and the ledger it belongs to can never describe different revisions.
    fn with_compaction(
        aggregate: ConversationAggregate,
        summary_text: &str,
        up_to_message_id: &str,
        summary_tokens: i64,
    ) -> ConversationAggregate {
        let compacted = aggregate
            .messages()
            .iter()
            .position(|m| m.id == up_to_message_id)
            .map(|index| index + 1)
            .unwrap_or(0) as i64;
        let original_tokens = (compacted * 10).max(1);
        let record = CompactionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: aggregate.id().clone(),
            summary_text: summary_text.to_string(),
            up_to_message_id: up_to_message_id.to_string(),
            original_message_count: compacted,
            original_tokens,
            summary_tokens,
            compression_ratio: (summary_tokens as f64 / original_tokens as f64).min(1.0),
            created_at: Utc::now(),
        };
        ConversationAggregate::from_persistence(
            aggregate.conversation().clone(),
            aggregate.messages().to_vec(),
            aggregate.document_context().to_vec(),
            Some(record),
        )
    }

    #[test]
    fn compaction_replaces_the_folded_prefix_in_the_llm_context() {
        let aggregate = aggregate_with_messages(6);
        let boundary = aggregate.messages()[3].id.clone();
        let aggregate = with_compaction(aggregate, "distilled past", &boundary, 5);

        // Every message is still present for display...
        assert_eq!(aggregate.messages().len(), 6);
        // ...but the context carries the summary plus only what follows it.
        let live: Vec<_> = aggregate
            .live_messages()
            .iter()
            .map(|m| m.content.as_str())
            .collect();
        assert_eq!(live, vec!["message 4", "message 5"]);

        // The summary itself is rendered by the context assembler, which is the
        // one place a prompt is built; the aggregate's job is only to say which
        // messages are still carried raw and what the summary text is.
        let preamble = aggregate
            .context_preamble()
            .expect("a compaction has a preamble");
        assert!(preamble.contains("distilled past"));
        // And it must say who wrote it. This preamble used to read only
        // "[Earlier conversation, summarized]", which let the QA and
        // context-window paths hand a model's paraphrase to another model as
        // though it were the transcript.
        assert!(
            preamble.contains("written by a model"),
            "a summary presented without that caveat reads as something the user said"
        );
    }

    #[test]
    fn a_stale_boundary_keeps_the_history_instead_of_erasing_it() {
        let aggregate = aggregate_with_messages(4);
        let boundary = aggregate.messages()[1].id.clone();
        let mut aggregate = with_compaction(aggregate, "summary", &boundary, 5);

        // The boundary message is deleted out from under the summary.
        aggregate.messages.retain(|m| m.id != boundary);

        // Falling open costs tokens; falling closed would silently drop the
        // entire live conversation from the model's context.
        assert_eq!(aggregate.live_messages().len(), 3);
    }

    #[test]
    fn test_conversation_serialization_camelcase() {
        let conv = Conversation {
            id: ConversationId::new(),
            title: "Test Conversation".to_string(),
            model_name: "claude-sonnet-4-5-20250929".to_string(),
            system_prompt: Some("You are helpful".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 5,
            total_tokens: 1000,
        };
        let json = serde_json::to_string(&conv).unwrap();
        assert!(
            json.contains("\"modelName\":"),
            "Expected modelName in JSON"
        );
        assert!(
            json.contains("\"systemPrompt\":"),
            "Expected systemPrompt in JSON"
        );
        assert!(
            json.contains("\"createdAt\":"),
            "Expected createdAt in JSON"
        );
        assert!(
            json.contains("\"updatedAt\":"),
            "Expected updatedAt in JSON"
        );
        assert!(
            json.contains("\"messageCount\":"),
            "Expected messageCount in JSON"
        );
        assert!(
            json.contains("\"totalTokens\":"),
            "Expected totalTokens in JSON"
        );
    }

    #[test]
    fn test_conversation_message_serialization_camelcase() {
        let msg = ConversationMessage {
            id: "msg-123".to_string(),
            conversation_id: ConversationId::new(),
            role: MessageRole::User,
            content: "Hello".to_string(),
            tokens: 10,
            created_at: Utc::now(),
            metadata: Some(r#"{"key":"value"}"#.to_string()),
            status: "completed".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(
            json.contains("\"conversationId\":"),
            "Expected conversationId in JSON"
        );
        assert!(
            json.contains("\"createdAt\":"),
            "Expected createdAt in JSON"
        );
    }

    #[test]
    fn test_document_reference_serialization_camelcase() {
        let doc_ref = DocumentReference {
            document_id: "doc-123".to_string(),
            chunk_id: Some("chunk-456".to_string()),
            relevance_score: Some(0.95),
            added_at: Utc::now(),
        };
        let json = serde_json::to_string(&doc_ref).unwrap();
        assert!(
            json.contains("\"documentId\":"),
            "Expected documentId in JSON"
        );
        assert!(json.contains("\"chunkId\":"), "Expected chunkId in JSON");
        assert!(
            json.contains("\"relevanceScore\":"),
            "Expected relevanceScore in JSON"
        );
        assert!(json.contains("\"addedAt\":"), "Expected addedAt in JSON");
    }
}
