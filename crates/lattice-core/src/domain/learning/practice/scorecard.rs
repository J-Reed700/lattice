//! Scorecards of coding sessions: rows scored 0 to max or marked as lacking evidence, each backed by evidence the app
//! checks against its own data (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 4.8).

use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Whether a row was scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ScoreStatus {
    Observed,
    InsufficientEvidence,
}

/// One checked fact behind a row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScorecardEvidence {
    /// An exact substring of that learner turn.
    Quote {
        #[serde(rename = "turnId")]
        turn_id: String,
        text: String,
    },
    /// Lines `start` to `end`, 1-based and inclusive, of `path` in that run's files.
    Lines {
        #[serde(rename = "runId")]
        run_id: String,
        path: String,
        start: u32,
        end: u32,
    },
    Run {
        #[serde(rename = "runId")]
        run_id: String,
    },
    Example {
        ordinal: u32,
    },
    Hint {
        ordinal: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScorecardRow {
    pub criterion_id: String,
    pub title: String,
    pub status: ScoreStatus,
    pub points: Option<u8>,
    pub max_points: u8,
    pub evidence: Vec<ScorecardEvidence>,
    pub feedback: String,
    pub next_step: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Scorecard {
    pub rows: Vec<ScorecardRow>,
}

impl Scorecard {
    /// The invariants every scorecard keeps: points iff observed, points at most the maximum, an observed row has
    /// evidence.
    pub fn validate(&self) -> Result<()> {
        Err(AppError::InvalidState("not built yet".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(status: ScoreStatus, points: Option<u8>, evidence: Vec<ScorecardEvidence>) -> ScorecardRow {
        ScorecardRow {
            criterion_id: "code".into(),
            title: "Code".into(),
            status,
            points,
            max_points: 4,
            evidence,
            feedback: "Clear.".into(),
            next_step: "Name the helper.".into(),
        }
    }

    fn card(row: ScorecardRow) -> Scorecard {
        Scorecard { rows: vec![row] }
    }

    fn some_evidence() -> Vec<ScorecardEvidence> {
        vec![ScorecardEvidence::Run {
            run_id: "run-1".into(),
        }]
    }

    #[test]
    fn an_observed_row_with_points_and_evidence_is_valid() {
        let scorecard = card(row(ScoreStatus::Observed, Some(3), some_evidence()));
        assert!(scorecard.validate().is_ok());
    }

    #[test]
    fn full_marks_are_valid() {
        let scorecard = card(row(ScoreStatus::Observed, Some(4), some_evidence()));
        assert!(scorecard.validate().is_ok());
    }

    #[test]
    fn zero_points_on_evidence_are_valid() {
        let scorecard = card(row(ScoreStatus::Observed, Some(0), some_evidence()));
        assert!(scorecard.validate().is_ok());
    }

    #[test]
    fn an_insufficient_row_with_no_points_is_valid() {
        let scorecard = card(row(ScoreStatus::InsufficientEvidence, None, vec![]));
        assert!(scorecard.validate().is_ok());
    }

    #[test]
    fn an_observed_row_without_points_is_refused() {
        let scorecard = card(row(ScoreStatus::Observed, None, some_evidence()));
        assert!(scorecard.validate().is_err());
    }

    #[test]
    fn an_insufficient_row_with_points_is_refused() {
        let scorecard = card(row(ScoreStatus::InsufficientEvidence, Some(0), vec![]));
        assert!(scorecard.validate().is_err());
    }

    #[test]
    fn points_above_the_maximum_are_refused() {
        let scorecard = card(row(ScoreStatus::Observed, Some(5), some_evidence()));
        assert!(scorecard.validate().is_err());
    }

    #[test]
    fn points_without_evidence_are_refused() {
        let scorecard = card(row(ScoreStatus::Observed, Some(2), vec![]));
        assert!(scorecard.validate().is_err());
    }

    #[test]
    fn a_row_that_can_give_no_points_is_refused() {
        let mut empty = row(ScoreStatus::InsufficientEvidence, None, vec![]);
        empty.max_points = 0;
        assert!(card(empty).validate().is_err());
    }

    #[test]
    fn one_bad_row_refuses_the_scorecard() {
        let scorecard = Scorecard {
            rows: vec![
                row(ScoreStatus::Observed, Some(3), some_evidence()),
                row(ScoreStatus::Observed, Some(2), vec![]),
            ],
        };
        assert!(scorecard.validate().is_err());
    }

    #[test]
    fn evidence_serializes_with_its_kind_and_camel_case_fields() {
        let quote = ScorecardEvidence::Quote {
            turn_id: "t1".into(),
            text: "a hash map".into(),
        };
        let json = serde_json::to_value(&quote).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({"kind": "quote", "turnId": "t1", "text": "a hash map"})
        );
        let lines = ScorecardEvidence::Lines {
            run_id: "r1".into(),
            path: "solution.py".into(),
            start: 2,
            end: 4,
        };
        let json = serde_json::to_value(&lines).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({"kind": "lines", "runId": "r1", "path": "solution.py", "start": 2, "end": 4})
        );
        let back: ScorecardEvidence = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, lines);
    }

    #[test]
    fn statuses_serialize_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&ScoreStatus::InsufficientEvidence).expect("serialize"),
            "\"insufficient_evidence\""
        );
        assert_eq!(
            serde_json::to_string(&ScoreStatus::Observed).expect("serialize"),
            "\"observed\""
        );
    }
}
