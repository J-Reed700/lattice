//! Wire types for the folder index.

use serde::{Deserialize, Serialize};

/// Where the open folder's index stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FolderIndexState {
    /// Walking the folder and comparing it with what is already indexed.
    Scanning,
    /// Embedding the files that are new or changed.
    Indexing,
    Ready,
    /// More indexable files than the cap; nothing was embedded.
    TooLarge,
    /// The filesystem root, the home folder or an ancestor of it, or a
    /// folder that holds Lattice's own data.
    Refused,
    /// No active embedding model.
    Unavailable,
    Error,
}

/// The index of one open folder. `index_root` differs from `root` when the
/// folder sits inside one that already has an index and that index is reused.
///
/// Files move when a save lands (a file counts once all its passages are in
/// the saved vectors file); passages move after every embedded batch, so
/// progress is told in passages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderIndexStatusDto {
    pub root: String,
    pub index_root: String,
    pub state: FolderIndexState,
    /// While scanning, the files found so far.
    pub files_total: u32,
    pub files_indexed: u32,
    pub passages_total: u32,
    pub passages_embedded: u32,
    /// Passages embedded per second, smoothed; `None` until a run has
    /// enough behind it to say.
    pub passages_per_second: Option<f32>,
    /// Seconds left at that rate; `None` when there is no rate yet.
    pub eta_seconds: Option<u32>,
    pub message: Option<String>,
}

impl FolderIndexStatusDto {
    pub fn new(root: &str, index_root: &str, state: FolderIndexState) -> Self {
        Self {
            root: root.to_string(),
            index_root: index_root.to_string(),
            state,
            files_total: 0,
            files_indexed: 0,
            passages_total: 0,
            passages_embedded: 0,
            passages_per_second: None,
            eta_seconds: None,
            message: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Whole percent of passages embedded; 0 with nothing to embed.
    pub fn percent(&self) -> u32 {
        if self.passages_total == 0 {
            return 0;
        }
        let share = u64::from(self.passages_embedded.min(self.passages_total)) * 100
            / u64::from(self.passages_total);
        u32::try_from(share).unwrap_or(100)
    }
}

/// What a folder's index holds, as the folders list shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FolderIndexSummaryState {
    /// Its last full run finished and everything found is embedded.
    Indexed,
    /// Some of it is embedded: a run was stopped before it finished.
    Partial,
    /// Open now and building; the numbers are live.
    Indexing,
    /// No index yet.
    NotIndexed,
    TooLarge,
    Refused,
    Error,
}

/// One folder's index for the folders list. For the open folder it is the
/// live status; for the rest, what the index on disk holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderIndexSummaryDto {
    pub state: FolderIndexSummaryState,
    pub files_total: u32,
    pub files_indexed: u32,
    pub passages_total: u32,
    pub passages_embedded: u32,
    /// Bytes on disk of the folder's own index directory; 0 when it has none.
    pub bytes: u64,
    /// The enclosing folder whose index this one reuses; `None` when it uses
    /// its own.
    pub index_root: Option<String>,
    /// While indexing: seconds left, when known.
    pub eta_seconds: Option<u32>,
    pub message: Option<String>,
}

impl FolderIndexSummaryDto {
    pub fn new(state: FolderIndexSummaryState) -> Self {
        Self {
            state,
            files_total: 0,
            files_indexed: 0,
            passages_total: 0,
            passages_embedded: 0,
            bytes: 0,
            index_root: None,
            eta_seconds: None,
            message: None,
        }
    }
}
