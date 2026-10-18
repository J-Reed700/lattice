//! Pictures in a chat: the ones an assistant answer shows inline and the
//! ones the reader attaches to a message.
//!
//! The conversation feature finds, stages, prepares, attaches and releases
//! pictures through this port; `features::web::images` implements it and the
//! `Container` exposes it, so the chat loop never reaches into the web
//! feature.
//! Design: docs/design/2026-10-17-chat-images-and-diagrams.md §7.2, §7.6;
//! docs/design/2026-10-17-chat-vision.md §10.3.

use async_trait::async_trait;

use crate::application::contracts::images::{
    ChatImageDto, FindImageInput, FindImageOutput, PreparedImageDto,
};
use crate::domain::models::vision::VisionGeometry;
use crate::shared::error::Result;

/// The `request_id` prefix of a picture the composer staged and no message
/// has claimed yet. Each staging call gets its own `draft:<ulid>`.
pub const DRAFT_REQUEST_PREFIX: &str = "draft:";

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

    /// Store a picture the reader attached, from bytes already validated.
    /// The row is leased to `scope.request_id` (a draft) until a message
    /// claims it; refs run `att-1…` within the draft.
    async fn store_attachment(
        &self,
        scope: &ImageTurnScope,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<ChatImageDto>;

    /// The picture fitted to `geometry` (within `max_tokens` when given),
    /// from the prepared cache or prepared now. `None` when the row is gone.
    async fn prepared(
        &self,
        image_id: &str,
        geometry: VisionGeometry,
        max_tokens: Option<u32>,
    ) -> Result<Option<PreparedImageDto>>;

    /// Rows the conversation holds for these ids, in the order given; ids of
    /// other conversations are dropped, not errors.
    async fn rows_for_ids(
        &self,
        conversation_id: &str,
        image_ids: &[String],
    ) -> Result<Vec<ChatImageDto>>;

    /// Claim rows for a persisted user message (`message_id` set,
    /// `request_id` rewritten to the turn's, refs renumbered `att-1…` in the
    /// order given). Rows another message already holds are left alone.
    /// Idempotent: claiming the same ids for the same message again returns
    /// the same list.
    async fn attach_ids_to_message(
        &self,
        conversation_id: &str,
        image_ids: &[String],
        message_id: &str,
        request_id: &str,
    ) -> Result<Vec<ChatImageDto>>;

    /// Release draft rows the composer discarded, with their files. Ignores
    /// rows a message already holds.
    async fn release_images(&self, conversation_id: &str, image_ids: &[String]) -> Result<()>;

    /// A new draft row with its own copy of the file, for "show to the model
    /// again" and for a regenerate that must keep its question's pictures.
    async fn restage(&self, conversation_id: &str, image_id: &str) -> Result<ChatImageDto>;
}
