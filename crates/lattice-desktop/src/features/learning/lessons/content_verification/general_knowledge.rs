//! Which unsupported lesson claims the machine proposes as general knowledge,
//! and the human resolutions a published claim may carry.
//!
//! A claim the saved references neither support nor contradict, without
//! invented specifics, is *proposed* as general knowledge: the claim waits for
//! the user, who accepts the label or not (§13 D1). Only an accepted label, or
//! the user's confirmation, lets a claim publish without a supporting passage.
//! The exceptions are where invented specifics concentrate: a number,
//! version, date or percentage that the claim's evidence does not give, or an
//! absolute. Those are plain unsupported claims, so repair removes or softens
//! them. Contradicted and unverified claims are never general knowledge.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §5.6, §13 D1.

use super::{ClaimVerdict, Finding};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// How a claim without a supporting passage was resolved by the user. Set on
/// the report's finding from the claim ledger at publication; never by the
/// machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(in crate::features::learning) enum Resolution {
    /// "Confirmed by you", with the user's note (required over a contradiction).
    Confirmed { at: i64, note: Option<String> },
    /// The general-knowledge label, accepted by the user.
    GeneralKnowledge { at: i64 },
}

/// Words that overstate what a source would say; their inflections included.
const ABSOLUTES: &[&str] = &[
    "always",
    "never",
    "must",
    "all",
    "every",
    "only",
    "guarantee",
    "guarantees",
    "guaranteed",
    "purely",
    "entirely",
];

/// The lowercase words of `text`.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

/// Tokens carrying a digit (numbers, versions, dates, percentages), without
/// surrounding punctuation or a percent sign.
fn figures(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split_whitespace()
        .map(|token| {
            token
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|token| token.chars().any(|c| c.is_ascii_digit()))
}

impl Finding {
    /// The machine's proposal: an unsupported claim without invented
    /// specifics may be general knowledge. Only the user accepts it.
    pub(super) fn proposes_general_knowledge(&self) -> bool {
        self.verdict == ClaimVerdict::Unsupported && !self.states_specifics()
    }

    /// May publish: supported by a passage, or resolved by the user.
    pub(super) fn publishable(&self) -> bool {
        self.verdict == ClaimVerdict::Supported || self.resolution.is_some()
    }

    /// Whether the claim states an absolute, or a figure or percentage none
    /// of its evidence passages gives.
    fn states_specifics(&self) -> bool {
        let given: HashSet<String> = self
            .evidence
            .iter()
            .flat_map(|passage| figures(&passage.text))
            .collect();
        let evidence_percent = self.evidence.iter().any(|passage| {
            passage.text.contains('%') || words(&passage.text).any(|word| word == "percent")
        });
        words(&self.statement).any(|word| {
            ABSOLUTES.contains(&word.as_str()) || (word == "percent" && !evidence_percent)
        }) || figures(&self.statement).any(|figure| !given.contains(&figure))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;
    use crate::features::learning::content_verification::EvidencePassage;

    fn finding(statement: &str, verdict: ClaimVerdict, evidence: &[&str]) -> Finding {
        Finding {
            unit: 0,
            quote: statement.into(),
            statement: statement.into(),
            verdict,
            reason: "The saved references do not state this.".into(),
            evidence: evidence
                .iter()
                .map(|text| EvidencePassage {
                    source_id: "source".into(),
                    text: (*text).into(),
                    start_byte: 0,
                    end_byte: text.len(),
                    retrieval_kind: "lexical_fallback".into(),
                    score: 1.0,
                    branch: None,
                    branch_score: 0.0,
                })
                .collect(),
            supporting_quote: None,
            resolution: None,
            trust: None,
        }
    }

    #[test]
    fn an_unsupported_claim_without_specifics_is_general_knowledge() {
        let claim = "Command handlers change state while query handlers read it.";
        assert!(finding(claim, ClaimVerdict::Unsupported, &[]).proposes_general_knowledge());
        for verdict in [
            ClaimVerdict::Supported,
            ClaimVerdict::Contradicted,
            ClaimVerdict::Unverified,
        ] {
            assert!(!finding(claim, verdict, &[]).proposes_general_knowledge());
        }
    }

    #[test]
    fn a_figure_the_evidence_does_not_give_stays_an_issue() {
        for claim in [
            "Read models serve 90% of requests.",
            "The pattern appeared in 2010.",
            "MediatR 12.1 dispatches notifications in parallel.",
            "A cluster needs 3 nodes.",
            "Most systems spend ninety percent of their time reading.",
        ] {
            assert!(
                !finding(claim, ClaimVerdict::Unsupported, &["Unrelated text."])
                    .proposes_general_knowledge(),
                "{claim}"
            );
        }
        // The same figures are general knowledge when the evidence gives them.
        assert!(finding(
            "MediatR 12.1 dispatches notifications in parallel.",
            ClaimVerdict::Unsupported,
            &["Version 12.1 changed how notifications are published."]
        )
        .proposes_general_knowledge());
        assert!(finding(
            "Read models serve 90% of requests.",
            ClaimVerdict::Unsupported,
            &["In that survey 90 percent of requests were reads."]
        )
        .proposes_general_knowledge());
    }

    /// D1: the machine only proposes general knowledge; publication accepts
    /// it once the user has, and never an unsupported figure left unresolved.
    #[test]
    fn publication_accepts_general_knowledge_but_not_an_unsupported_figure() {
        use crate::features::learning::dto::{
            LearningBlockDto, LearningBlockKind, PreparedLearningLesson,
        };
        let accepted = Some(Resolution::GeneralKnowledge { at: 1 });
        for (body, resolution, publishable) in [
            ("Small handlers are easier to test.", accepted.clone(), true),
            ("Small handlers are easier to test.", None, false),
            ("Small handlers cut defects by 40%.", None, false),
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

    #[test]
    fn an_absolute_stays_an_issue() {
        for word in ABSOLUTES {
            let claim = format!("Aggregates {word} enforce their own invariants.");
            assert!(
                !finding(&claim, ClaimVerdict::Unsupported, &[]).proposes_general_knowledge(),
                "{word}"
            );
        }
        // Matching is by whole word: "allows" and "overall" are not absolutes.
        assert!(finding(
            "The overall design allows separate read models.",
            ClaimVerdict::Unsupported,
            &[]
        )
        .proposes_general_knowledge());
    }
}
