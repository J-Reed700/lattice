//! Wire types for the Explorer: a folder on disk read live beside a chat.
//!
//! Paths inside a scope are relative, `/`-separated, with no leading `./`;
//! `""` is the root itself. Roots on the wire are canonical absolute paths.

use super::index::dto::FolderIndexSummaryDto;
use serde::{Deserialize, Serialize};

/// A folder the user chose, after canonicalisation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerRootDto {
    pub root: String,
    /// The folder's own name, for the scope bar.
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ExplorerEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerEntryDto {
    pub name: String,
    pub path: String,
    pub kind: ExplorerEntryKind,
    /// Bytes on disk; `None` for directories.
    pub size: Option<u64>,
    /// Matched by `.gitignore` (or sits inside something that is). The tree
    /// shows these dimmed; search and the model's tools leave them out.
    pub ignored: bool,
}

/// One level of a directory: directories first, then files, each sorted
/// case-insensitively.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerListingDto {
    pub root: String,
    pub path: String,
    pub entries: Vec<ExplorerEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerFileDto {
    pub root: String,
    pub path: String,
    /// `None` when the file is binary or over the size cap.
    pub text: Option<String>,
    pub line_count: u32,
    pub size_bytes: u64,
    pub language: Option<String>,
    pub binary: bool,
    pub too_large: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerSearchMatchDto {
    pub path: String,
    /// 1-based.
    pub line: u32,
    /// 1-based, in characters.
    pub column: u32,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerSearchResultDto {
    pub matches: Vec<ExplorerSearchMatchDto>,
    /// A bound was hit (results, files, bytes or time), so there may be more.
    pub truncated: bool,
    pub files_scanned: u32,
}

/// 1-based, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerLineRangeDto {
    pub start_line: u32,
    pub end_line: u32,
}

/// What the user is looking at when they send: the open file and the
/// selected lines. Rides along on each Explorer turn.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerFocusDto {
    pub open_path: Option<String>,
    pub selection: Option<ExplorerLineRangeDto>,
}

/// One folder in the Explorer's folders list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerFolderDto {
    pub root: String,
    /// The display name; the folder's own name unless renamed.
    pub name: String,
    pub pinned: bool,
    /// RFC 3339.
    pub added_at: String,
    /// RFC 3339.
    pub last_opened_at: String,
    /// The folder is still on disk.
    pub exists: bool,
    /// Explorer threads bound to this folder.
    pub thread_count: u32,
    pub index: FolderIndexSummaryDto,
    /// The folder's system prompt; `None` when it has none and its space's
    /// prompt applies.
    pub instructions: Option<String>,
    /// The space its threads belong to; General unless one was chosen.
    pub space_id: String,
}

/// The folders list, with the home folder so paths under it can be shown
/// as `~/…`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerFolderListDto {
    pub home: Option<String>,
    pub folders: Vec<ExplorerFolderDto>,
}
