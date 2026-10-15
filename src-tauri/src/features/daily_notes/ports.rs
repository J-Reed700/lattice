//! What journal pages need from the features built on them.
//!
//! Daily notes are the record: conversations cite and link them, and the vault
//! mirrors them to markdown files. So daily notes names neither feature. Each
//! thing it needs from them is a port declared here, and the composition root
//! binds the port to the feature that provides it.

use async_trait::async_trait;

use crate::shared::error::Result;

/// A cited web page's text as a conversation archived it when it cited the
/// page: the copy a journal citation is recovered from.
#[derive(Debug, Clone)]
pub struct ArchivedWebPage {
    pub title: Option<String>,
    pub content: String,
    pub fetched_at: Option<String>,
    pub truncated: bool,
}

/// The archives of the conversations a page links. Older pages saved a web
/// citation's URL without its text; while a linked conversation still holds
/// its copy of that page, the citation can be filled from it.
#[async_trait]
pub trait LinkedConversationArchive: Send + Sync {
    /// The copy `conversation_id` archived of `url`, if it kept one.
    async fn archived_web_page(
        &self,
        conversation_id: &str,
        url: &str,
    ) -> Result<Option<ArchivedWebPage>>;
}

/// A copy of the journal kept outside the database, such as the vault's
/// markdown mirror. A database trigger queues the copy's outbox entry with
/// every note write, so the database stays the canonical record and nothing
/// waits on the copy; these calls only wake it, so it need not wait for its
/// next recovery pass.
pub trait NoteProjection: Send + Sync {
    /// A note was created or edited.
    fn note_saved(&self);
    /// A note was deleted.
    fn note_deleted(&self);
}
