//! Citation chains (§8): which links in a supporting passage to follow, in
//! what order, to reach the source a secondary page repeats.

use serde::{Deserialize, Serialize};

/// What is known about a link's origin for the claim's subject, for
/// ranking: the owner's page first, then a known authoritative one, then
/// the nearest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStanding {
    Unknown,
    Authoritative,
    Owner,
}

/// One link worth following, with why it ranked where it did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationCandidate {
    pub url: String,
    pub anchor: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub standing: LinkStanding,
    /// Bytes between the link and the passage; 0 inside it. A footnote
    /// target counts as 0: the passage names it.
    pub distance: usize,
}
