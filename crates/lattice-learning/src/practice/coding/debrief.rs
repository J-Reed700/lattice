//! Coding session debrief: scorecard grading, evidence verification, and persistence
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.8 and 7.3).

use super::hints::{CheckedCodeHint, CodeHintRung};
use super::problem::Setting;
use super::session::{CodingExampleDto, StageTimingDto};
use crate::practice::practical_dto::{LearningPracticalRunDto, LearningSimulationTurnDto};
use crate::recall::study_dto::StudyRating;
use async_trait::async_trait;
use lattice_core::domain::learning::practice::scorecard::{
    EvidenceItem, EvidenceKind, Scorecard, ScorecardCriterionId, ScorecardRow, ScorecardStatus,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternDebriefDto {
    pub pattern_id: String,
    pub name: String,
    pub family: String,
    pub recognition_cue: String,
    pub rating: StudyRating,
    pub due_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodingDebriefDto {
    pub session_id: String,
    pub scorecard: Scorecard,
    pub patterns: Vec<PatternDebriefDto>,
    pub approach_summary: String,
    pub target_complexity: String,
    pub reference_solution: String,
    pub uncaught_misreadings: Vec<String>,
    pub stage_timings: Vec<StageTimingDto>,
    pub run_count: u32,
    pub hints_used: Vec<CodeHintRung>,
    pub setting: Setting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvidence {
    pub session_id: String,
    pub draft_code: String,
    pub draft_line_count: u32,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub runs: Vec<LearningPracticalRunDto>,
    pub examples: Vec<CodingExampleDto>,
    pub hints: Vec<CheckedCodeHint>,
}

#[async_trait]
pub trait Debriefer: Send + Sync {
    async fn debrief(&self, evidence: &SessionEvidence) -> Result<Scorecard>;
}

pub struct CodingDebriefer {
    pub grader_model: String,
}

impl CodingDebriefer {
    pub fn new(grader_model: impl Into<String>) -> Self {
        Self {
            grader_model: grader_model.into(),
        }
    }

    /// Verifies evidence items against `evidence`, rejecting/stripping invalid ones
    /// and enforcing InsufficientEvidence for criteria lacking learner actions.
    pub fn verify_scorecard(
        &self,
        _evidence: &SessionEvidence,
        _raw: Scorecard,
    ) -> Result<Scorecard> {
        Err(AppError::InvalidState(
            "CodingDebriefer::verify_scorecard stub".into(),
        ))
    }
}

#[async_trait]
impl Debriefer for CodingDebriefer {
    async fn debrief(&self, _evidence: &SessionEvidence) -> Result<Scorecard> {
        Err(AppError::InvalidState(
            "CodingDebriefer::debrief stub".into(),
        ))
    }
}

/// Saves a scorecard to `learning_scorecards`.
pub async fn save_scorecard(
    _pool: &SqlitePool,
    _scorecard: &Scorecard,
) -> Result<()> {
    Err(AppError::InvalidState("save_scorecard stub".into()))
}

/// Loads a scorecard from `learning_scorecards` for `session_id`.
pub async fn load_scorecard(
    _pool: &SqlitePool,
    _session_id: &str,
) -> Result<Option<Scorecard>> {
    Err(AppError::InvalidState("load_scorecard stub".into()))
}

/// Emits evidence events into `learning_evidence_events` for each scorecard row.
pub async fn emit_evidence_events(
    _pool: &SqlitePool,
    _program_id: &str,
    _scorecard: &Scorecard,
    _at: i64,
) -> Result<()> {
    Err(AppError::InvalidState("emit_evidence_events stub".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::practical_dto::LearningSimulationSpeaker;

    fn sample_evidence(learner_text: &str, line_count: u32) -> SessionEvidence {
        SessionEvidence {
            session_id: "test_sess".to_string(),
            draft_code: "def solve(nums):\n    return 42\n".to_string(),
            draft_line_count: line_count,
            turns: vec![LearningSimulationTurnDto {
                id: "turn_1".to_string(),
                ordinal: 0,
                speaker: LearningSimulationSpeaker::Learner,
                content: learner_text.to_string(),
                citations: vec![],
                model_name: None,
                created_at: 0,
            }],
            runs: vec![],
            examples: vec![],
            hints: vec![],
        }
    }

    #[test]
    fn test_verify_quote_must_be_learner_turn_substring() {
        let debriefer = CodingDebriefer::new("grader_v1");
        let evidence = sample_evidence("I will use a two pointers approach from both ends", 10);

        let valid_quote = EvidenceItem {
            kind: EvidenceKind::Quote,
            text: "two pointers approach".to_string(),
            start_line: None,
            end_line: None,
        };
        let fabricated_quote = EvidenceItem {
            kind: EvidenceKind::Quote,
            text: "I will use Dijkstra algorithm with min-heap".to_string(),
            start_line: None,
            end_line: None,
        };

        let raw = Scorecard {
            session_id: "test_sess".to_string(),
            grader_model: "grader_v1".to_string(),
            rows: vec![
                ScorecardRow {
                    criterion_id: "problem_solving".to_string(),
                    title: "Problem solving".to_string(),
                    description: "desc".to_string(),
                    status: ScorecardStatus::Observed,
                    points: Some(4),
                    max_points: 4,
                    evidence: vec![valid_quote],
                    feedback: "Good approach".to_string(),
                    next_step: "step".to_string(),
                },
                ScorecardRow {
                    criterion_id: "communication".to_string(),
                    title: "Communication".to_string(),
                    description: "desc".to_string(),
                    status: ScorecardStatus::Observed,
                    points: Some(3),
                    max_points: 4,
                    evidence: vec![fabricated_quote],
                    feedback: "fb".to_string(),
                    next_step: "step".to_string(),
                },
            ],
        };

        let verified = debriefer.verify_scorecard(&evidence, raw).expect("verify");
        // Valid quote must be retained
        assert_eq!(verified.rows[0].evidence.len(), 1);
        assert_eq!(verified.rows[0].evidence[0].text, "two pointers approach");

        // Fabricated quote must be stripped or rejected
        assert!(
            verified.rows[1].evidence.is_empty(),
            "Fabricated quote not present in turns must be stripped"
        );
    }

    #[test]
    fn test_verify_lines_within_draft_bounds() {
        let debriefer = CodingDebriefer::new("grader_v1");
        let evidence = sample_evidence("typing code", 10);

        let valid_lines = EvidenceItem {
            kind: EvidenceKind::Lines,
            text: "return 42".to_string(),
            start_line: Some(2),
            end_line: Some(5),
        };
        let out_of_bounds_lines = EvidenceItem {
            kind: EvidenceKind::Lines,
            text: "phantom line".to_string(),
            start_line: Some(15),
            end_line: Some(20),
        };

        let raw = Scorecard {
            session_id: "test_sess".to_string(),
            grader_model: "grader_v1".to_string(),
            rows: vec![ScorecardRow {
                criterion_id: "code".to_string(),
                title: "Code".to_string(),
                description: "desc".to_string(),
                status: ScorecardStatus::Observed,
                points: Some(3),
                max_points: 4,
                evidence: vec![valid_lines, out_of_bounds_lines],
                feedback: "fb".to_string(),
                next_step: "step".to_string(),
            }],
        };

        let verified = debriefer.verify_scorecard(&evidence, raw).expect("verify");
        // Only lines within 1..=draft_line_count (10) may survive
        assert_eq!(verified.rows[0].evidence.len(), 1);
        assert_eq!(verified.rows[0].evidence[0].start_line, Some(2));
    }

    #[test]
    fn test_verify_run_example_hint_must_exist() {
        let debriefer = CodingDebriefer::new("grader_v1");
        let mut evidence = sample_evidence("explanation", 10);
        evidence.hints.push(CheckedCodeHint {
            rung: CodeHintRung::Nudge,
            content: "Look at the constraints".to_string(),
            reason: "Help with bounds".to_string(),
            lines: vec![],
        });

        let valid_hint_ev = EvidenceItem {
            kind: EvidenceKind::Hint,
            text: "Look at the constraints".to_string(),
            start_line: None,
            end_line: None,
        };
        let fabricated_run_ev = EvidenceItem {
            kind: EvidenceKind::Run,
            text: "run_nonexistent_99".to_string(),
            start_line: None,
            end_line: None,
        };

        let raw = Scorecard {
            session_id: "test_sess".to_string(),
            grader_model: "grader_v1".to_string(),
            rows: vec![ScorecardRow {
                criterion_id: "testing".to_string(),
                title: "Testing".to_string(),
                description: "desc".to_string(),
                status: ScorecardStatus::Observed,
                points: Some(2),
                max_points: 4,
                evidence: vec![valid_hint_ev, fabricated_run_ev],
                feedback: "fb".to_string(),
                next_step: "step".to_string(),
            }],
        };

        let verified = debriefer.verify_scorecard(&evidence, raw).expect("verify");
        assert_eq!(verified.rows[0].evidence.len(), 1);
        assert_eq!(verified.rows[0].evidence[0].kind, EvidenceKind::Hint);
    }

    #[test]
    fn test_silence_yields_insufficient_evidence_with_none_points() {
        let debriefer = CodingDebriefer::new("grader_v1");
        // No learner turns (silent), no runs, no examples
        let evidence = SessionEvidence {
            session_id: "silent_sess".to_string(),
            draft_code: "".to_string(),
            draft_line_count: 0,
            turns: vec![],
            runs: vec![],
            examples: vec![],
            hints: vec![],
        };

        let raw = Scorecard {
            session_id: "silent_sess".to_string(),
            grader_model: "grader_v1".to_string(),
            rows: vec![
                ScorecardRow {
                    criterion_id: "communication".to_string(),
                    title: "Communication".to_string(),
                    description: "desc".to_string(),
                    status: ScorecardStatus::Observed,
                    points: Some(0), // Raw model tried to give 0 points for silence
                    max_points: 4,
                    evidence: vec![],
                    feedback: "Didn't speak".to_string(),
                    next_step: "Speak next time".to_string(),
                },
                ScorecardRow {
                    criterion_id: "testing".to_string(),
                    title: "Testing".to_string(),
                    description: "desc".to_string(),
                    status: ScorecardStatus::Observed,
                    points: Some(0), // Raw model tried to give 0 points for no tests
                    max_points: 4,
                    evidence: vec![],
                    feedback: "Didn't test".to_string(),
                    next_step: "Test next time".to_string(),
                },
            ],
        };

        let verified = debriefer.verify_scorecard(&evidence, raw).expect("verify");

        // Silence on Communication must be InsufficientEvidence with points: None (NEVER Some(0))
        let comm_row = verified
            .rows
            .iter()
            .find(|r| r.criterion_id == "communication")
            .expect("communication row");
        assert_eq!(comm_row.status, ScorecardStatus::InsufficientEvidence);
        assert_eq!(
            comm_row.points, None,
            "Silence must never score 0 points: points must be None"
        );

        // No runs/examples on Testing must be InsufficientEvidence with points: None
        let test_row = verified
            .rows
            .iter()
            .find(|r| r.criterion_id == "testing")
            .expect("testing row");
        assert_eq!(test_row.status, ScorecardStatus::InsufficientEvidence);
        assert_eq!(
            test_row.points, None,
            "Lack of testing evidence must never score 0 points: points must be None"
        );
    }

    #[tokio::test]
    async fn test_learning_scorecards_round_trip_persistence() {
        let pool = crate::test_support::pool().await.expect("test pool");
        let session_id = uuid::Uuid::new_v4().to_string();
        let prog = crate::test_support::fixture();
        crate::repository::LearningRepository::new(pool.clone())
            .create(&prog)
            .await
            .expect("insert program");

        let scorecard = Scorecard {
            session_id: session_id.clone(),
            grader_model: "test-grader".to_string(),
            rows: vec![
                ScorecardRow {
                    criterion_id: "problem_solving".to_string(),
                    title: "Problem solving".to_string(),
                    description: "Understood problem".to_string(),
                    status: ScorecardStatus::Observed,
                    points: Some(3),
                    max_points: 4,
                    evidence: vec![EvidenceItem {
                        kind: EvidenceKind::Quote,
                        text: "I will use two pointers".to_string(),
                        start_line: None,
                        end_line: None,
                    }],
                    feedback: "Good strategy".to_string(),
                    next_step: "Watch edge cases".to_string(),
                },
                ScorecardRow {
                    criterion_id: "communication".to_string(),
                    title: "Communication".to_string(),
                    description: "Explained thinking".to_string(),
                    status: ScorecardStatus::InsufficientEvidence,
                    points: None,
                    max_points: 4,
                    evidence: vec![],
                    feedback: "Little spoken explanation".to_string(),
                    next_step: "Talk through edge cases".to_string(),
                },
            ],
        };

        save_scorecard(&pool, &scorecard)
            .await
            .expect("save scorecard");

        let loaded = load_scorecard(&pool, &session_id)
            .await
            .expect("load scorecard")
            .expect("scorecard should exist");

        assert_eq!(loaded.session_id, scorecard.session_id);
        assert_eq!(loaded.rows.len(), 2);
        assert_eq!(loaded.rows[0].criterion_id, "problem_solving");
        assert_eq!(loaded.rows[0].status, ScorecardStatus::Observed);
        assert_eq!(loaded.rows[0].points, Some(3));
        assert_eq!(loaded.rows[0].evidence.len(), 1);

        assert_eq!(loaded.rows[1].criterion_id, "communication");
        assert_eq!(loaded.rows[1].status, ScorecardStatus::InsufficientEvidence);
        assert_eq!(loaded.rows[1].points, None);
    }

    #[tokio::test]
    async fn test_emit_evidence_events_into_db() {
        let pool = crate::test_support::pool().await.expect("test pool");
        let session_id = uuid::Uuid::new_v4().to_string();
        let prog = crate::test_support::fixture();
        let program_id = prog.summary.id.clone();
        let now = 1_700_000_000_000i64;

        crate::repository::LearningRepository::new(pool.clone())
            .create(&prog)
            .await
            .expect("insert program");

        let scorecard = Scorecard {
            session_id: session_id.clone(),
            grader_model: "test-grader".to_string(),
            rows: vec![ScorecardRow {
                criterion_id: "code".to_string(),
                title: "Code".to_string(),
                description: "Idiomatic Python".to_string(),
                status: ScorecardStatus::Observed,
                points: Some(4),
                max_points: 4,
                evidence: vec![],
                feedback: "Clean solution".to_string(),
                next_step: "Benchmark".to_string(),
            }],
        };

        emit_evidence_events(&pool, &program_id, &scorecard, now)
            .await
            .expect("emit events");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM learning_evidence_events WHERE source_id = ?",
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .expect("count events");

        assert_eq!(count, 1);
    }
}
