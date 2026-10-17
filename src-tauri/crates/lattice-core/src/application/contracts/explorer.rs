//! What the Explorer's chat turn is told about the folder view: the open file
//! and the selected lines.

use serde::{Deserialize, Serialize};

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
