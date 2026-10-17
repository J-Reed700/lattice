//! Pictures an assistant answer shows inline.
//!
//! The conversation feature finds, attaches and releases pictures through
//! this port; `features::web::images` implements it and the `Container`
//! exposes it, so the chat loop never reaches into the web feature.
//! Design: docs/design/2026-10-17-chat-images-and-diagrams.md §7.2, §7.6.

use async_trait::async_trait;

use crate::application::contracts::images::{ChatImageDto, FindImageInput, FindImageOutput};
use crate::shared::error::Result;

/// The turn a `find_image` call belongs to. Rows stored for it are leased
/// by `request_id` until the answer persists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageTurnScope {
    pub conversation_id: String,
    pub request_id: String,
}

/// A message copied into another conversation (a fork or a tangent), with
/// the ids of the pictures its copied `metadata.images` still names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedMessage {
    pub message_id: String,
    pub image_ids: Vec<String>,
}

#[async_trait]
pub trait ChatImagesPort: Send + Sync {
    /// Search, fetch, validate and store up to `input.limit` pictures for this
    /// turn. Network work stops at `deadline` (the turn's own, never a fixed
    /// timeout). Returns what the model sees and what the window sees.
    async fn find_and_store(
        &self,
        scope: &ImageTurnScope,
        input: FindImageInput,
        deadline: Option<std::time::Instant>,
    ) -> Result<(FindImageOutput, Vec<ChatImageDto>)>;

    /// Attach the request's rows whose ref is in `referenced_refs` to the
    /// persisted message; release the rest. Returns the attached list.
    async fn attach_to_message(
        &self,
        request_id: &str,
        message_id: &str,
        referenced_refs: &[String],
    ) -> Result<Vec<ChatImageDto>>;

    /// A turn that failed or was stopped before persisting its answer.
    async fn release_request(&self, request_id: &str) -> Result<()>;

    async fn release_message(&self, message_id: &str) -> Result<()>;

    async fn release_conversation(&self, conversation_id: &str) -> Result<()>;

    async fn list_for_message(&self, message_id: &str) -> Result<Vec<ChatImageDto>>;

    /// Delete the picture files no row claims, and return how many went.
    /// Messages deleted without `release_message` — a truncated thread, a
    /// regenerated answer — take their rows with them by cascade; this frees
    /// their files then rather than at the next start's sweep. A turn's
    /// leased pictures still have their rows, so they are kept.
    async fn remove_unclaimed_files(&self) -> Result<u64>;

    /// Give each copied message its own pictures: a new row and file per
    /// named picture, attached to the copy in `conversation_id`, with the
    /// same ref, caption and source. Deleting the original then leaves the
    /// copy's pictures intact. Returns, per message, the copies made; a
    /// picture whose original is gone or whose file could not be copied is
    /// left out.
    async fn copy_for_messages(
        &self,
        conversation_id: &str,
        copies: &[CopiedMessage],
    ) -> Result<Vec<(String, Vec<ChatImageDto>)>>;
}
