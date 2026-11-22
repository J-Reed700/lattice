//! Workspace notes as the vault mirror reads and writes them.

use async_trait::async_trait;

use crate::application::contracts::notes::{NoteTimestampRecord, WorkspaceNoteRecord};
use crate::shared::error::Result;

#[async_trait]
pub trait WorkspaceNotesPort: Send + Sync {
    /// The note with this id, or `NotFound`.
    async fn get(&self, note_id: &str) -> Result<WorkspaceNoteRecord>;

    /// The note's timestamps, or `None` when no such note exists.
    async fn get_timestamp(&self, note_id: &str) -> Result<Option<NoteTimestampRecord>>;

    /// Delete a note whose file left the vault, unless an app write to it is
    /// still waiting to be mirrored. Returns whether the row was deleted.
    async fn delete_from_vault_if_unqueued(&self, note_id: &str) -> Result<bool>;
}
