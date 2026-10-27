//! SQLite-backed CRUD for conversations, messages and document references,
//! through the mapper layer so domain entities stay apart from database
//! models.

mod branching_port;
mod chat_port;
mod conversations;
mod document_references;
mod fork;
mod knowledge;
mod memory;
mod memory_port;
mod memory_recall;
mod memory_semantic;
mod memory_vectors;
mod messages;
mod port;
mod pruning;
mod tangents;

#[cfg(test)]
mod tests;
mod workspace;

use sqlx::SqlitePool;

/// SQLite implementation of the conversation repository.
#[derive(Clone)]
pub struct ConversationRepository {
    pool: SqlitePool,
    memory_embedding: Option<std::sync::Arc<dyn lattice_core::application::ports::EmbeddingPort>>,
}

impl ConversationRepository {
    /// Creates a conversation repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            memory_embedding: None,
        }
    }
}

impl ConversationRepository {
    pub fn with_memory_embedding(
        mut self,
        embedding: Option<std::sync::Arc<dyn lattice_core::application::ports::EmbeddingPort>>,
    ) -> Self {
        self.memory_embedding = embedding;
        self
    }
}
impl std::fmt::Debug for ConversationRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConversationRepository")
            .finish_non_exhaustive()
    }
}
