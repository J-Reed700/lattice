//! `ConversationMemoryPort` / `ConversationMemoryReadPort` adapters.
//!
//! Thin delegation to the inherent methods in [`super::memory`] and
//! [`super::memory_recall`], so the application layer depends on a trait it can
//! fake while the SQL stays owned by the repository.

use super::ConversationRepository;
use crate::application::ports::conversation_memory::{
    CommittedMemorySnapshot, ConversationMemoryPort, ConversationMemoryReadPort,
    MemoryCommitCandidate, MemoryCommitError, MemoryCommitPreconditions, RecallCandidates,
    ResolvedSpan, SourcePage, SourceReadLimits, SourceSpanRef,
};
use crate::domain::conversation_memory::{
    ConversationMemoryState, MemoryId, MemoryItem, MemorySnapshot, SourceMessage,
};
use crate::shared::error::Result;
use async_trait::async_trait;

#[async_trait]
impl ConversationMemoryPort for ConversationRepository {
    async fn load_snapshot(&self, conversation_id: &str) -> Result<MemorySnapshot> {
        self.load_memory_snapshot(conversation_id).await
    }

    async fn page_source_messages(
        &self,
        conversation_id: &str,
        after_sequence: i64,
        through_sequence: i64,
        limits: SourceReadLimits,
    ) -> Result<SourcePage> {
        self.page_memory_source_messages(conversation_id, after_sequence, through_sequence, limits)
            .await
    }

    async fn read_source_spans(
        &self,
        conversation_id: &str,
        spans: &[SourceSpanRef],
        limits: SourceReadLimits,
    ) -> Result<Vec<ResolvedSpan>> {
        self.read_memory_source_spans(conversation_id, spans, limits)
            .await
    }

    async fn commit_memory(
        &self,
        preconditions: &MemoryCommitPreconditions,
        candidate: &MemoryCommitCandidate,
    ) -> std::result::Result<CommittedMemorySnapshot, MemoryCommitError> {
        ConversationRepository::commit_memory(self, preconditions, candidate).await
    }

    async fn record_memory_error(&self, conversation_id: &str, code: &str) -> Result<()> {
        ConversationRepository::record_memory_error(self, conversation_id, code).await
    }

    async fn mark_rebuild_required(&self, conversation_id: &str, code: &str) -> Result<()> {
        self.mark_memory_rebuild_required(conversation_id, code)
            .await
    }

    async fn page_inactive_items(
        &self,
        conversation_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<MemoryItem>> {
        self.page_inactive_memory_items(conversation_id, offset, limit)
            .await
    }

    async fn load_state(&self, conversation_id: &str) -> Result<ConversationMemoryState> {
        self.load_memory_state(conversation_id).await
    }
}

#[async_trait]
impl ConversationMemoryReadPort for ConversationRepository {
    async fn search_source_messages(
        &self,
        conversation_id: &str,
        query: &str,
        exact_terms: &[String],
        limit: usize,
    ) -> Result<RecallCandidates> {
        self.search_memory_source_messages(conversation_id, query, exact_terms, limit)
            .await
    }

    async fn read_adjacent_turns(
        &self,
        conversation_id: &str,
        sequence: i64,
        before: usize,
        after: usize,
        limits: SourceReadLimits,
    ) -> Result<Vec<SourceMessage>> {
        self.read_memory_adjacent_turns(conversation_id, sequence, before, after, limits)
            .await
    }

    async fn read_messages(
        &self,
        conversation_id: &str,
        message_ids: &[String],
        limits: SourceReadLimits,
    ) -> Result<Vec<SourceMessage>> {
        self.read_memory_messages(conversation_id, message_ids, limits)
            .await
    }

    async fn read_sequence_range(
        &self,
        conversation_id: &str,
        from_sequence: i64,
        to_sequence: i64,
        limits: SourceReadLimits,
    ) -> Result<SourcePage> {
        self.read_memory_sequence_range(conversation_id, from_sequence, to_sequence, limits)
            .await
    }

    async fn search_memory_labels(
        &self,
        conversation_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryId>> {
        self.search_memory_item_labels(conversation_id, query, limit)
            .await
    }
}
