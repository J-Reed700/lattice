//! What a lesson statement's citation is: how it uses its sources and which
//! sentences of them its chips open.
use super::{EvidencePassage, Finding};
use lattice_core::application::contracts::replies::AnchorDto;
use lattice_core::domain::citation::{Anchor, Relation};

/// How a statement is cited: its relation to its sources, whether its own
/// text is drawn as a quotation, and the passages its chips open.
pub(super) struct Cited<'f> {
    pub(super) relation: Relation,
    /// The statement's own lesson text is drawn as a quotation.
    #[allow(dead_code)]
    pub(super) quoted: bool,
    /// The passages the chips open, in the order the anchors come, each with its
    /// anchors; an empty list opens the passage whole.
    #[allow(dead_code)]
    pub(super) on: Vec<(&'f EvidencePassage, Vec<Anchor>)>,
}

/// The citation of a finding's statement; none when it gets no chip.
pub(super) fn cited(_finding: &Finding) -> Option<Cited<'_>> {
    None
}

/// `anchor` as the wire carries it, its words cut from `passage`; none when
/// the anchor does not lie in that passage.
pub(super) fn anchor_dto(_passage: &EvidencePassage, _anchor: &Anchor) -> Option<AnchorDto> {
    None
}

#[cfg(test)]
#[path = "support_tests.rs"]
mod tests;
