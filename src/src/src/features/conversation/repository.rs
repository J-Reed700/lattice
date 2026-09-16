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

mod conversations;
mod document_references;
mod fork;
mod messages;
mod port;
mod pruning;
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
#[derive(Debug, Clone)]
pub struct ConversationRepository {
    pool: SqlitePool,
}

impl ConversationRepository {
    /// Create a new conversation repository.
    ///
    /// # Arguments
    ///
    /// * `pool` - SQLite connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
