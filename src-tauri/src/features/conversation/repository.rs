//! Conversation Repository Implementation
//!
//! SQLite-backed CRUD for conversations, messages, and document
//! references. All DB ops go through the mapper layer to keep domain
//! entities decoupled from database models.
//!
//! # Architecture note
//!
//! This used to implement a `ConversationRepositoryPort` trait. The
//! trait had one implementor (this struct) and zero `dyn` consumers —
//! a Java/C# 'Header Interface' anti-pattern in Rust. Methods are now
//! plain inherent methods. If polymorphism is ever needed, extract
//! the trait at that point.

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
mod workspace;

#[cfg(test)]
mod tests;

use sqlx::SqlitePool;

/// SQLite implementation of conversation repository.
///
/// Handles persistence of conversation metadata, messages, and document references.
/// Uses mappers to convert between domain entities and database models.
///
/// # Thread Safety
///
/// This repository is thread-safe through SQLitePool's internal connection management.
#[derive(Clone)]
pub struct ConversationRepository {
    pool: SqlitePool,
    memory_embedding: Option<std::sync::Arc<dyn crate::application::ports::EmbeddingPort>>,
}

impl ConversationRepository {
    /// Create a new conversation repository.
    ///
    /// # Arguments
    ///
    /// * `pool` - SQLite connection pool
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
        embedding: Option<std::sync::Arc<dyn crate::application::ports::EmbeddingPort>>,
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
