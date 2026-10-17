//! The chat turn's records port over this repository.
use super::ConversationRepository;
use crate::features::conversation::chat::ports::{ChatRecords, MemoryVector};
use crate::features::conversation::workspace_dto::ConversationWebSourceSnapshotDto;
use async_trait::async_trait;
use lattice_core::application::contracts::search::{CorpusDocument, CorpusPassage};
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::domain::conversation::ChatModelChoice;
use lattice_core::shared::error::Result;
use std::collections::HashSet;
use std::sync::Arc;

#[async_trait]
impl ChatRecords for ConversationRepository {
    async fn model_choice(&self, conversation_id: &str) -> Result<ChatModelChoice> {
        ConversationRepository::model_choice(self, conversation_id).await
    }

    async fn set_model_choice(
        &self,
        conversation_id: &str,
        choice: &ChatModelChoice,
    ) -> Result<()> {
        ConversationRepository::set_model_choice(self, conversation_id, choice).await
    }

    async fn retrieval_document_scope(
        &self,
        conversation_id: &str,
    ) -> Result<Option<(String, HashSet<String>)>> {
        ConversationRepository::retrieval_document_scope(self, conversation_id).await
    }

    async fn retrieval_catalog(
        &self,
        allowed_ids: &HashSet<String>,
    ) -> Result<Vec<CorpusDocument>> {
        ConversationRepository::retrieval_catalog(self, allowed_ids).await
    }

    async fn retrieval_mean_chunk_chars(
        &self,
        allowed_ids: &HashSet<String>,
    ) -> Result<Option<usize>> {
        ConversationRepository::retrieval_mean_chunk_chars(self, allowed_ids).await
    }

    async fn retrieval_openings(
        &self,
        selected_ids: &[String],
        allowed_ids: &HashSet<String>,
        chunks_per_document: usize,
    ) -> Result<Vec<CorpusPassage>> {
        ConversationRepository::retrieval_openings(
            self,
            selected_ids,
            allowed_ids,
            chunks_per_document,
        )
        .await
    }

    async fn retrieval_section_passages(
        &self,
        identifiers: &[String],
        allowed: &HashSet<String>,
    ) -> Result<Vec<CorpusPassage>> {
        ConversationRepository::retrieval_section_passages(self, identifiers, allowed).await
    }

    async fn retrieval_locations(
        &self,
        chunk_ids: &[String],
    ) -> Result<std::collections::HashMap<String, (Option<String>, Option<u32>, bool)>> {
        ConversationRepository::retrieval_locations(self, chunk_ids).await
    }

    async fn retrieval_neighbors(
        &self,
        chunk_id: &str,
        allowed: &HashSet<String>,
    ) -> Result<Vec<CorpusPassage>> {
        ConversationRepository::retrieval_neighbors(self, chunk_id, allowed).await
    }

    async fn web_source_snapshot(
        &self,
        conversation_id: &str,
        url: &str,
    ) -> Result<Option<ConversationWebSourceSnapshotDto>> {
        self.conversation_web_source_snapshot(conversation_id.to_string(), url.to_string())
            .await
    }

    async fn store_web_source_snapshot(
        &self,
        conversation_id: &str,
        url: &str,
        title: Option<String>,
        content: String,
        truncated: bool,
    ) -> Result<()> {
        self.store_conversation_web_source_snapshot(
            conversation_id.to_string(),
            url.to_string(),
            title,
            content,
            truncated,
        )
        .await
        .map(|_| ())
    }

    async fn add_cited_web_source(
        &self,
        conversation_id: &str,
        url: String,
        title: Option<String>,
        excerpt: Option<String>,
        score: Option<f32>,
    ) -> Result<()> {
        self.add_conversation_web_source(conversation_id.to_string(), url, title, excerpt, score)
            .await
            .map(|_| ())
    }

    async fn persist_memory_vector(&self, vector: MemoryVector<'_>) -> Result<u64> {
        ConversationRepository::persist_memory_vector(
            self,
            vector.conversation_id,
            vector.vector_id,
            vector.message_id,
            vector.role,
            vector.content,
            vector.embedding,
            vector.dimension,
            vector.embedding_model,
            vector.created_at,
        )
        .await
    }

    fn with_recall_embedding(
        &self,
        embedding: Option<Arc<dyn EmbeddingPort>>,
    ) -> Arc<dyn ChatRecords> {
        Arc::new(self.clone().with_memory_embedding(embedding))
    }
}
