//! The user's rewrite of a published section: which sentences changed, and
//! what each claim on the old text becomes.

use std::ops::Range;

use lattice_core::domain::learning::claim_review::{ClaimId, ClaimLedgerEntry};

/// A run of sentences that differ between the old and new text, as index
/// ranges into each text's sentences; either side may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hunk {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

/// The hunks between `old` and `new`, sentence by sentence.
pub(super) fn sentence_diff(_old: &str, _new: &str) -> Vec<Hunk> {
    Vec::new()
}

/// What a rewrite of one field of a section does to the claims on it.
#[derive(Debug, Default)]
pub(super) struct FieldPlan {
    pub kept: Vec<ClaimId>,
    /// Claims on changed sentences, each with the index into `written` of the
    /// sentence that takes its place.
    pub edited: Vec<(ClaimId, usize)>,
    /// Claims on sentences deleted with nothing in their place.
    pub removed: Vec<ClaimId>,
    /// Every sentence of the new text that the old text did not have.
    pub written: Vec<Written>,
}

/// A sentence the user wrote, and the claim that stands for it.
#[derive(Debug)]
pub(super) struct Written {
    pub text: String,
    pub span: (usize, usize),
    pub successor: ClaimId,
}

pub(super) fn plan(
    _claims: &[ClaimLedgerEntry],
    _unit: usize,
    _field: &str,
    _old: &str,
    _new: &str,
) -> FieldPlan {
    FieldPlan::default()
}
