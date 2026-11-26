//! A reply as the frontend draws it: text between references.

use crate::domain::citation::Relation;
use serde::{Deserialize, Serialize};

/// A line range a reference names inside what it points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LinesDto {
    pub first: u32,
    pub last: u32,
}

/// How a sentence uses a source, as the frontend names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RelationDto {
    Quotes,
    Restates,
    Summarizes,
    Infers,
    Agrees,
    Backs,
    Cites,
    Contradicts,
}

impl From<Relation> for RelationDto {
    fn from(relation: Relation) -> Self {
        match relation {
            Relation::Quotes => Self::Quotes,
            Relation::Restates => Self::Restates,
            Relation::Summarizes => Self::Summarizes,
            Relation::Infers => Self::Infers,
            Relation::Agrees => Self::Agrees,
            Relation::Backs => Self::Backs,
            Relation::Cites => Self::Cites,
            Relation::Contradicts => Self::Contradicts,
        }
    }
}

/// A span of a source version's saved text, in UTF-8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ByteSpanDto {
    pub start: usize,
    pub end: usize,
}

/// The words a citation rests on, with where they stand when that is known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AnchorDto {
    pub exact: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<ByteSpanDto>,
}

/// How a marker uses its source, and the words it rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SupportDto {
    pub relation: RelationDto,
    pub anchors: Vec<AnchorDto>,
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
    /// How the marked sentence uses its source; none when nothing says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support: Option<SupportDto>,
}

/// One piece of a reply, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ReplyPieceDto {
    Text { text: String },
    Reference { reference: ReplyReferenceDto },
    Quoted { text: String },
}
