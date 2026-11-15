//! A reply as the frontend draws it: text between references.

use serde::{Deserialize, Serialize};

/// A line range a reference names inside what it points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LinesDto {
    pub first: u32,
    pub last: u32,
}

/// A marker of a reply that resolved, as the frontend draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReplyReferenceDto {
    /// The handle as written, `[s12]` or `[f2:10-20]`.
    pub written: String,
    /// `source` or `file`.
    pub kind: String,
    /// The handle's own number.
    pub number: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<LinesDto>,
    /// What a kind drawn by name shows: a file's path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// One piece of a reply, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ReplyPieceDto {
    Text { text: String },
    Reference { reference: ReplyReferenceDto },
}
