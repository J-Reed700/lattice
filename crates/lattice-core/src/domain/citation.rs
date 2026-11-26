//! How a sentence uses a source: the spans it rests on and the relation that
//! follows from what each part of it rests on.

use serde::{Deserialize, Serialize};

/// A span of one source version's saved text, in UTF-8 bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Anchor {
    pub source_version_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// How a sentence uses a source, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// The sentence is the source's own words.
    Quotes,
    /// One source sentence says all of it, in other words.
    Restates,
    /// Several source sentences together say it.
    Summarizes,
    /// The source's sentences entail it; none says it.
    Infers,
    /// The writer's own reasoning, checked for consistency with the source.
    Agrees,
    /// Checked and supported, but the check named no sentence.
    Backs,
    /// The writer cited it; nothing checked the sentence against it.
    Cites,
    /// The source says otherwise.
    Contradicts,
}

/// What a part of a statement rests on its anchors as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grounds {
    /// The anchors state the part.
    Stated,
    /// The anchors entail the part; none states it.
    Follows,
}

/// The relation a statement has to its sources, from the anchors of its parts,
/// and the anchors it rests on in source order, each once. A part without an
/// anchor is left out; none when no part has one.
pub fn relation_of(_parts: &[(Grounds, &[Anchor])]) -> Option<(Relation, Vec<Anchor>)> {
    None
}

#[cfg(test)]
#[path = "citation_tests.rs"]
mod tests;
