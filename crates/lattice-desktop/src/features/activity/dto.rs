//! What the renderer's activity view reads.

use lattice_core::application::ports::{ActivityProgress, ActivityState, ActivityTarget};
use serde::Serialize;

/// The event that tells the renderer to read the snapshot again. Job
/// changes come on `jobs://status`, downloads and the library indexer on
/// their own events; this one carries what has no event of its own: model
/// requests queueing, models loading, the index turn order.
pub const CHANGED_EVENT: &str = "activity://changed";

/// One live item, with why it waits and what it holds put into words.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntryDto {
    pub id: String,
    /// The sort of work: a job kind, or `model.requests`, `model.loading`,
    /// `embedding.loading`, `download`, `library.indexing`.
    pub kind: String,
    pub title: String,
    pub state: ActivityState,
    /// What it is doing, or plainly why it waits or holds.
    pub detail: Option<String>,
    pub progress: Option<ActivityProgress>,
    /// Units of `progress` per second, measured.
    pub per_second: Option<f64>,
    /// Seconds left at the measured pace; absent without one.
    pub seconds_left: Option<u64>,
    /// What it holds or uses: "Using the embedding model".
    pub holding: Option<String>,
    pub target: Option<ActivityTarget>,
    /// When it started, or was asked for while it waits (Unix ms).
    pub since: Option<i64>,
    /// When a job put off by an outage tries again (Unix ms).
    pub retry_at: Option<i64>,
}

/// Everything running, paused or waiting now: running first, then paused,
/// then waiting, each oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySnapshotDto {
    pub items: Vec<ActivityEntryDto>,
    /// How many of `items` are running.
    pub running: u32,
}
