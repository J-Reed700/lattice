//! Frozen copies for learning references, through the port the learning
//! feature holds (`ReferenceSnapshotPort`). Copies asked for here belong to
//! the library conversation, so deleting a chat never takes a reference's
//! frozen page with it; a copy of the same page kept for a chat is reused.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §5.4.

use async_trait::async_trait;
use lattice_core::application::ports::{ReferenceSnapshot, ReferenceSnapshotPort};
use lattice_core::shared::error::Result;
use lattice_core::shared::http::normalize_web_source_url;

use super::repository::SnapshotRow;
use super::{WebPageSnapshotService, WebPageSnapshotStatus};

fn snapshot_of(row: &SnapshotRow) -> ReferenceSnapshot {
    match (row.status(), row.sha256.as_deref()) {
        (WebPageSnapshotStatus::Ready, Some(sha256)) => ReferenceSnapshot::Ready {
            sha256: sha256.to_owned(),
        },
        (WebPageSnapshotStatus::Ready, None) => ReferenceSnapshot::Failed {
            reason: "the frozen copy has no recorded hash".into(),
        },
        (WebPageSnapshotStatus::Pending, _) => ReferenceSnapshot::Pending,
        (WebPageSnapshotStatus::Failed, _) => ReferenceSnapshot::Failed {
            reason: row
                .error_message
                .clone()
                .or_else(|| row.error_code.clone())
                .unwrap_or_else(|| "the capture failed".into()),
        },
    }
}

#[async_trait]
impl ReferenceSnapshotPort for WebPageSnapshotService {
    async fn request(&self, url: &str, retry: bool) -> Result<ReferenceSnapshot> {
        let repository = &self.shared().repository;
        let library = repository.library_conversation().await?;
        if !retry {
            let normalized = normalize_web_source_url(url.trim());
            if let Some(row) = repository.find_for_viewer(&library, &normalized).await? {
                if row.status() != WebPageSnapshotStatus::Failed {
                    return Ok(snapshot_of(&row));
                }
            }
        }
        let row = self.ensure_queued(&library, url, retry).await?;
        Ok(snapshot_of(&row))
    }

    async fn status(&self, url: &str) -> Result<Option<ReferenceSnapshot>> {
        let normalized = normalize_web_source_url(url.trim());
        Ok(self
            .shared()
            .repository
            .find_for_viewer("", &normalized)
            .await?
            .map(|row| snapshot_of(&row)))
    }
}
