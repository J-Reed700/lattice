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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderIndexStatusDto {
    pub root: String,
    pub index_root: String,
    pub state: FolderIndexState,
    pub files_total: u32,
    pub files_indexed: u32,
    pub chunks: u32,
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
            chunks: 0,
            message: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}
