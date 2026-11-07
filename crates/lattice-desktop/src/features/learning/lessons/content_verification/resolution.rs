//! What the user resolved about a claim, and what a report may publish with:
//! a claim a passage supports, or one the user resolved. Design:
//! docs/design/2026-10-16-lesson-claim-review.md.

use super::{ClaimVerdict, Finding};
use lattice_core::domain::learning::claim_review::UnverifiedReason;
use serde::{Deserialize, Serialize};

/// How a claim without a supporting passage was resolved by the user. Set on
/// the report's finding from the claim ledger at publication; never by the
/// machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(in crate::features::learning) enum Resolution {
    /// "Confirmed by you", with the user's note (required over a contradiction).
    Confirmed { at: i64, note: Option<String> },
    /// The machine could not settle it and the lesson publishes with it,
    /// shown to the learner as not verified.
    Unverified { reason: UnverifiedReason },
}

impl Finding {
    /// May publish: supported by a passage, or resolved by the user.
    pub(super) fn publishable(&self) -> bool {
        self.verdict == ClaimVerdict::Supported || self.resolution.is_some()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    /// The user's resolution is what lets an unsupported claim publish.
    #[test]
    fn publication_accepts_a_claim_the_user_resolved_but_not_an_unsupported_one() {
        use crate::features::learning::dto::{
            LearningBlockDto, LearningBlockKind, PreparedLearningLesson,
        };
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
                blocks: vec![LearningBlockDto {
                    kind: LearningBlockKind::Explanation,
                    title: "Handlers".into(),
                    body: body.into(),
                    source_ids: vec![],
                    rubric: vec![],
                }],
                questions: vec![],
                keys: vec![],
                verification: None,
            };
            let mut report =
                crate::features::learning::content_verification::tests::attest("lesson", &lesson);
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
