//! The model checks the readers make (course evidence index §5), each decision
//! keeping its own evidence binding and durable receipt. A statement is read
//! part by part against the passages its own sentence cites (`bridge`), or
//! against the course's first page for it by relevance (`walk`), and a claim
//! neither supports goes to web research. Requests hold one section's claims
//! while the window holds them with their answers (`requests`), and a page of
//! the course is sized by what the job's answers cost (`page_budget`). What a
//! reading makes of a finding is `outcome`'s (claim classes design §3.2).
use super::*;
use lattice_core::application::services::claim_verification::LocatedClaim;

mod bridge;
mod claims;
mod outcome;
mod page_budget;
mod requests;
mod walk;

pub(super) use bridge::{applies, bridge_checker, part_wise};
pub(super) use claims::{attribution_page, compare, Comparison, NO_PASSAGES};
#[cfg(test)]
pub(in crate::features::learning) use outcome::ContraryPassage;
pub(super) use outcome::{alone, page_finding};
pub(in crate::features::learning) use outcome::{
    same_passage, CheckedPart, Pass, SupportedElsewhere, SupportingPassage, TwoPass,
};
pub(in crate::features::learning) use page_budget::{page_budget, AnswerSize};
use requests::finish;
pub(super) use requests::{group, labelled_passages, pack};
pub(in crate::features::learning) use walk::Reach;
pub(super) use walk::{page_of, page_one, within_page, PageBudget, Sizing};

/// The most output a claim check may write, which the checker is built
/// with: the largest an answer can be.
pub(in crate::features::learning) const CHECKER_OUTPUT_TOKENS: u32 = 512;

/// The page budget a page of the course is sized by after what the job's
/// checks have cost so far, as the verification report says it: its tokens, or
/// that no measured pace gives one.
pub(super) fn page_budget_note(llm: &dyn LLMPort, retained: &ClaimChecks) -> String {
    PageBudget::of(
        llm,
        *retained
            .answers
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
    )
    .text()
}

/// Each passage's label, by source version (`provenance_gate::passage_labels`).
pub(super) type Labels = HashMap<String, String>;

/// Tells that one comparison finished, so a caller can count it while the
/// other comparisons of its request are pending.
type Answered<'a> = dyn Fn(Pass, &CheckedClaim) + Sync + 'a;

pub(super) struct PendingClaim {
    pub(super) ordinal: usize,
    pub(super) key: String,
    pub(super) receipt_key: String,
    pub(super) unit: usize,
    pub(super) claim: Claim,
    /// The claim's statement as the checker reads it (see [`as_checked`]).
    pub(super) checked: String,
    pub(super) evidence: Vec<EvidencePassage>,
    pub(super) pass: Pass,
}

/// `claim` as the checker reads it: each "Source N" of the lesson's text
/// written as that source's title, so the index cannot be read as passage N.
/// Its key names this text, so a check of the bare index is not restored.
pub(super) fn as_checked(
    claim: &Claim,
    citations: &Citations,
    references: &ReferenceCollection<'_>,
) -> Claim {
    Claim {
        quote: claim.quote.clone(),
        statement: citations.label(&claim.statement, references),
    }
}
