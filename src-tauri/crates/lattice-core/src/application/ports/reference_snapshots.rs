//! Frozen copies of pages saved as learning references.
//!
//! The web feature keeps frozen copies of pages (single-file, inert HTML,
//! hashed on disk); the learning feature asks for one when the user attaches
//! a link as evidence and records the copy's hash on the source version, so
//! the reader can open exactly the page that was checked.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §5.4.

use async_trait::async_trait;

use crate::shared::error::Result;

/// Where a page's frozen copy stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceSnapshot {
    /// Queued or being captured; the copy's hash is known once it is ready.
    Pending,
    /// Captured; `sha256` is the hash of the frozen file.
    Ready { sha256: String },
    /// The capture failed; the saved text still stands.
    Failed { reason: String },
}

#[async_trait]
pub trait ReferenceSnapshotPort: Send + Sync {
    /// Queues a frozen copy of `url` unless one is kept already, and says
    /// where it stands. `retry` queues a failed capture again (the user's
    /// "Capture again").
    async fn request(&self, url: &str, retry: bool) -> Result<ReferenceSnapshot>;

    /// Where the frozen copy of `url` stands, without queueing one. `None`
    /// when no copy was ever asked for.
    async fn status(&self, url: &str) -> Result<Option<ReferenceSnapshot>>;
}
