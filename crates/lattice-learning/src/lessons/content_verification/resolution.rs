//! What the user resolved about a claim, and what a report may publish with:
//! a claim a passage supports, or one the user resolved. Design:
//! docs/design/2026-10-16-lesson-claim-review.md.

use super::{ClaimVerdict, Finding};
use lattice_core::domain::learning::claim_review::{ClaimId, ClaimState, Class, UnverifiedReason};
use serde::{Deserialize, Serialize};

/// How a claim without a supporting passage was resolved by the user. Set on
/// the report's finding from the claim ledger at publication; never by the
/// machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Resolution {
    /// "Confirmed by you", with the user's note (required over a contradiction).
    Confirmed { at: i64, note: Option<String> },
    /// The machine could not settle it and the lesson publishes with it,
    /// shown to the learner as not verified.
    Unverified { reason: UnverifiedReason },
}

/// How a claim's state reads to the learner.
pub(super) enum Standing {
    /// The machine verified it, found it consistent or passed it as common
    /// knowledge.
    Verified,
    /// The user confirmed it, or it was published not verified.
    Resolved(Resolution),
    /// The sentence is gone: nothing is told of it.
    Gone,
    /// The claim is open, waiting or reopened.
    Unsettled,
}

/// The one mapping from a claim's state to the learner's resolution, used at
/// publication and when the learner view is read. `claim` gives the state of
/// a successor.
pub(super) fn standing<'a>(
    state: &ClaimState,
    claim: &dyn Fn(&ClaimId) -> Option<&'a ClaimState>,
) -> Standing {
    match state {
        ClaimState::Confirmed { at, note, .. } => Standing::Resolved(Resolution::Confirmed {
            at: *at,
            note: note.clone(),
        }),
        ClaimState::Unverified { reason, .. } => {
            Standing::Resolved(Resolution::Unverified { reason: *reason })
        }
        state if state.is_verified() => Standing::Verified,
        ClaimState::Removed { .. }
        | ClaimState::Edited {
            successor: None, ..
        } => Standing::Gone,
        ClaimState::Edited {
            successor: Some(successor),
            ..
        } => claim(successor).map_or(Standing::Gone, |state| standing(state, claim)),
        ClaimState::Unchecked
        | ClaimState::Open { .. }
        | ClaimState::Queued { .. }
        | ClaimState::Reopened { .. } => Standing::Unsettled,
    }
}

/// A claim as the ledger places it: its sentence's unit and quote, and where
/// it stands.
pub(super) struct Placed<'a> {
    pub id: &'a str,
    pub unit: usize,
    pub quote: &'a str,
    pub state: &'a ClaimState,
}

/// What a claim that is no finding of the pass, on the same unit and quote as
/// a finding, adds to it: a claim the machine ended whose sentence still
/// stands is published not verified, and the finding on that sentence carries
/// its resolution. Publication and the learner view both read it here.
pub(super) fn ended_mark<'a>(
    claims: impl IntoIterator<Item = Placed<'a>>,
    is_finding: &dyn Fn(&str) -> bool,
    unit: usize,
    quote: &str,
) -> Option<Resolution> {
    claims
        .into_iter()
        .filter(|claim| !is_finding(claim.id))
        .find_map(|claim| match claim.state {
            ClaimState::Unverified { reason, .. } if claim.unit == unit && claim.quote == quote => {
                Some(Resolution::Unverified { reason: *reason })
            }
            _ => None,
        })
}

impl Finding {
    /// May publish: supported by a passage, or resolved by the user.
    pub(super) fn publishable(&self) -> bool {
        self.verdict == ClaimVerdict::Supported || self.resolution.is_some()
    }

    /// A claim the learner is told about: every statement but one sorted Internal.
    pub(super) fn is_claim(&self) -> bool {
        self.class
            .is_none_or(|classed| classed.class != Class::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The user's resolution is what lets an unsupported claim publish.
    #[test]
    fn publication_accepts_a_claim_the_user_resolved_but_not_an_unsupported_one() {
        use crate::dto::{LearningBlockDto, LearningBlockKind, PreparedLearningLesson};
        let confirmed = Some(Resolution::Confirmed { at: 1, note: None });
        for (body, resolution, publishable) in [
            (
                "Small handlers are easier to test.",
                confirmed.clone(),
                true,
            ),
            (
                "Small handlers are easier to test.",
                Some(Resolution::Unverified {
                    reason: UnverifiedReason::Unsupported,
                }),
                true,
            ),
            ("Small handlers are easier to test.", None, false),
        ] {
            let lesson = PreparedLearningLesson {
                blocks: vec![LearningBlockDto::new(
                    LearningBlockKind::Explanation,
                    "Handlers".into(),
                    body.into(),
                    vec![],
                    vec![],
                )],
                questions: vec![],
                keys: vec![],
                verification: None,
            };
            let mut report = crate::content_verification::tests::attest("lesson", &lesson);
            report.findings[0].verdict = ClaimVerdict::Unsupported;
            report.findings[0].resolution = resolution.clone();
            assert_eq!(
                report.validate("lesson", &lesson).is_ok(),
                publishable,
                "{body} ({resolution:?})"
            );
        }
    }
}
