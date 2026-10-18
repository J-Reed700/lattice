//! Persistence boundary for where an imported document belongs.
//!
//! Two independent axes:
//!
//! - **Space membership** — which of the user's spaces a filed document shows
//!   up in. Many spaces per document, set by the import and by the user.
//! - **Conversation ownership** — whether the document is a file attached to
//!   one chat rather than filed in the library at all. At most one owner, and
//!   an owned document is invisible to the library, to the other chats, and to
//!   vault-wide search until [`DocumentScopePort::set_conversation_owner`]
//!   clears it.

use crate::shared::error::Result;

#[async_trait::async_trait]
pub trait DocumentScopePort: Send + Sync {
    async fn space_exists(&self, id: &str) -> Result<bool>;
    async fn conversation_exists(&self, id: &str) -> Result<bool>;
    /// Assign all documents atomically and idempotently. Empty input is a no-op.
    async fn assign_documents(&self, document_ids: &[String], space_id: &str) -> Result<()>;

    /// Mark these documents as owned by one conversation, or, with `None`,
    /// release them into the library.
    ///
    /// Atomic and idempotent, like [`Self::assign_documents`]: a half-stamped
    /// batch would leave some of one message's attachments in the library.
    async fn set_conversation_owner(
        &self,
        document_ids: &[String],
        conversation_id: Option<&str>,
    ) -> Result<()>;

    /// The conversation that owns this document, or `None` when it is filed in
    /// the library (or does not exist).
    async fn conversation_owner(&self, document_id: &str) -> Result<Option<String>>;

    /// The documents this conversation owns, in insertion order.
    async fn documents_owned_by_conversation(&self, conversation_id: &str) -> Result<Vec<String>>;

    /// Documents whose owning conversation no longer exists.
    ///
    /// Nothing can reach these — they are hidden from the library and from
    /// every conversation's scope — so the startup sweep deletes them. They
    /// appear when a conversation is removed by a path that did not delete its
    /// attachments first.
    async fn orphaned_conversation_owned_documents(&self) -> Result<Vec<String>>;
}
