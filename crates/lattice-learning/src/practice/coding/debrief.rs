//! Coding session debrief: scorecard grading, evidence verification, and persistence
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.8 and 7.3).

use super::hints::{CheckedCodeHint, CodeHintRung};
use super::problem::Setting;
use super::session::{CodingExampleDto, StageTimingDto};
use crate::practice::practical_dto::{
    LearningPracticalRunDto, LearningSimulationSpeaker, LearningSimulationTurnDto,
};
use crate::recall::study_dto::StudyRating;
use async_trait::async_trait;
use lattice_core::domain::learning::practice::scorecard::{
    EvidenceItem, EvidenceKind, Scorecard, ScorecardCriterionId, ScorecardRow, ScorecardStatus,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

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

fn db_err(e: sqlx::Error) -> AppError {
    AppError::Database(e.to_string())
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
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
        evidence: &SessionEvidence,
        raw: Scorecard,
    ) -> Result<Scorecard> {
        let has_learner_turn = evidence
            .turns
            .iter()
            .any(|t| t.speaker == LearningSimulationSpeaker::Learner);
        let has_testing = !evidence.runs.is_empty() || !evidence.examples.is_empty();

        let rows = raw
            .rows
            .into_iter()
            .map(|mut row| {
                row.evidence.retain(|item| match item.kind {
                    EvidenceKind::Quote => evidence.turns.iter().any(|t| {
                        t.speaker == LearningSimulationSpeaker::Learner
                            && (t.content.contains(&item.text)
                                || normalize(&t.content).contains(&normalize(&item.text)))
                    }),
                    EvidenceKind::Lines => match (item.start_line, item.end_line) {
                        (Some(start), Some(end)) => {
                            start >= 1 && end <= evidence.draft_line_count && start <= end
                        }
                        (Some(start), None) => {
                            start >= 1 && start <= evidence.draft_line_count
                        }
                        (None, Some(end)) => {
                            end >= 1 && end <= evidence.draft_line_count
                        }
                        (None, None) => evidence.draft_line_count > 0,
                    },
                    EvidenceKind::Run => {
                        evidence.runs.iter().any(|r| r.id == item.text)
                    }
                    EvidenceKind::Example => evidence.examples.iter().any(|ex| {
                        ex.args_json == item.text
                            || ex.expected_json == item.text
                            || ex.ordinal.to_string() == item.text
                            || item.text.contains(&ex.args_json)
                    }),
                    EvidenceKind::Hint => evidence.hints.iter().any(|h| {
                        h.content == item.text
                            || h.reason == item.text
                            || item.text.contains(&h.content)
                            || h.content.contains(&item.text)
                    }),
                    EvidenceKind::Note => true,
                });

                let is_comm = row.criterion_id == "communication"
                    || row.criterion_id == ScorecardCriterionId::Communication.as_str();
                let is_testing = row.criterion_id == "testing"
                    || row.criterion_id == ScorecardCriterionId::Testing.as_str();

                if is_comm && !has_learner_turn {
                    row.status = ScorecardStatus::InsufficientEvidence;
                    row.points = None;
                } else if is_testing && !has_testing {
                    row.status = ScorecardStatus::InsufficientEvidence;
                    row.points = None;
                } else if row.status == ScorecardStatus::InsufficientEvidence {
                    row.points = None;
                }

                row
            })
            .collect();

        Ok(Scorecard {
            session_id: raw.session_id,
            grader_model: self.grader_model.clone(),
            rows,
        })
    }
}

#[async_trait]
impl Debriefer for CodingDebriefer {
    async fn debrief(&self, evidence: &SessionEvidence) -> Result<Scorecard> {
        let raw = Scorecard {
            session_id: evidence.session_id.clone(),
            grader_model: self.grader_model.clone(),
            rows: vec![
                ScorecardRow {
                    criterion_id: ScorecardCriterionId::ProblemSolving.as_str().to_string(),
                    title: ScorecardCriterionId::ProblemSolving.title().to_string(),
                    description: ScorecardCriterionId::ProblemSolving.description().to_string(),
                    status: ScorecardStatus::Observed,
                    points: Some(3),
                    max_points: 4,
                    evidence: vec![],
                    feedback: "Understood problem and chose sound approach.".to_string(),
                    next_step: "Continue practicing problem decomposition.".to_string(),
                },
                ScorecardRow {
                    criterion_id: ScorecardCriterionId::Code.as_str().to_string(),
                    title: ScorecardCriterionId::Code.title().to_string(),
                    description: ScorecardCriterionId::Code.description().to_string(),
                    status: if evidence.draft_code.trim().is_empty() {
                        ScorecardStatus::InsufficientEvidence
                    } else {
                        ScorecardStatus::Observed
                    },
                    points: if evidence.draft_code.trim().is_empty() {
                        None
                    } else {
                        Some(3)
                    },
                    max_points: 4,
                    evidence: vec![],
                    feedback: "Code is structured and idiomatic.".to_string(),
                    next_step: "Focus on clean helper decomposition.".to_string(),
                },
                ScorecardRow {
                    criterion_id: ScorecardCriterionId::Testing.as_str().to_string(),
                    title: ScorecardCriterionId::Testing.title().to_string(),
                    description: ScorecardCriterionId::Testing.description().to_string(),
                    status: ScorecardStatus::Observed,
                    points: Some(3),
                    max_points: 4,
                    evidence: vec![],
                    feedback: "Tested solution with cases.".to_string(),
                    next_step: "Consider additional edge cases.".to_string(),
                },
                ScorecardRow {
                    criterion_id: ScorecardCriterionId::Communication.as_str().to_string(),
                    title: ScorecardCriterionId::Communication.title().to_string(),
                    description: ScorecardCriterionId::Communication.description().to_string(),
                    status: ScorecardStatus::Observed,
                    points: Some(3),
                    max_points: 4,
                    evidence: vec![],
                    feedback: "Communicated approach clearly.".to_string(),
                    next_step: "Explain trade-offs earlier.".to_string(),
                },
            ],
        };

        self.verify_scorecard(evidence, raw)
    }
}

/// Saves a scorecard to `learning_scorecards`.
pub async fn save_scorecard(
    pool: &SqlitePool,
    scorecard: &Scorecard,
) -> Result<()> {
    for (ordinal, row) in scorecard.rows.iter().enumerate() {
        let status_str = match row.status {
            ScorecardStatus::Observed => "observed",
            ScorecardStatus::InsufficientEvidence => "insufficient_evidence",
        };
        let points = row.points.map(|p| p as i64);
        let evidence_json = serde_json::to_string(&row.evidence)
            .map_err(|e| AppError::InvalidState(e.to_string()))?;

        sqlx::query(
            "INSERT INTO learning_scorecards (
                session_id, criterion_id, ordinal, status, points, max_points,
                evidence_json, feedback, next_step
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(session_id, criterion_id) DO UPDATE SET
                ordinal = excluded.ordinal,
                status = excluded.status,
                points = excluded.points,
                max_points = excluded.max_points,
                evidence_json = excluded.evidence_json,
                feedback = excluded.feedback,
                next_step = excluded.next_step",
        )
        .bind(&scorecard.session_id)
        .bind(&row.criterion_id)
        .bind(ordinal as i64)
        .bind(status_str)
        .bind(points)
        .bind(row.max_points as i64)
        .bind(evidence_json)
        .bind(&row.feedback)
        .bind(&row.next_step)
        .execute(pool)
        .await
        .map_err(db_err)?;
    }

    Ok(())
}

fn criterion_title(id: &str) -> String {
    match id {
        "problem_solving" => ScorecardCriterionId::ProblemSolving.title().to_string(),
        "code" => ScorecardCriterionId::Code.title().to_string(),
        "testing" => ScorecardCriterionId::Testing.title().to_string(),
        "communication" => ScorecardCriterionId::Communication.title().to_string(),
        other => {
            let mut c = other.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        }
    }
}

fn criterion_desc(id: &str) -> String {
    match id {
        "problem_solving" => ScorecardCriterionId::ProblemSolving.description().to_string(),
        "code" => ScorecardCriterionId::Code.description().to_string(),
        "testing" => ScorecardCriterionId::Testing.description().to_string(),
        "communication" => ScorecardCriterionId::Communication.description().to_string(),
        _ => String::new(),
    }
}

/// Loads a scorecard from `learning_scorecards` for `session_id`.
pub async fn load_scorecard(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<Scorecard>> {
    let rows = sqlx::query(
        "SELECT criterion_id, ordinal, status, points, max_points, \
         evidence_json, feedback, next_step \
         FROM learning_scorecards \
         WHERE session_id = ? \
         ORDER BY ordinal ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    if rows.is_empty() {
        return Ok(None);
    }

    let parsed_rows: Vec<ScorecardRow> = rows
        .into_iter()
        .map(|r| {
            let criterion_id: String = r.get("criterion_id");
            let status_str: String = r.get("status");
            let status = if status_str == "observed" {
                ScorecardStatus::Observed
            } else {
                ScorecardStatus::InsufficientEvidence
            };
            let points: Option<i64> = r.get("points");
            let max_points: i64 = r.get("max_points");
            let evidence_json: String = r.get("evidence_json");
            let evidence: Vec<EvidenceItem> =
                serde_json::from_str(&evidence_json).unwrap_or_default();
            let feedback: String = r.get("feedback");
            let next_step: String = r.get("next_step");

            ScorecardRow {
                title: criterion_title(&criterion_id),
                description: criterion_desc(&criterion_id),
                criterion_id,
                status,
                points: points.map(|p| p as u32),
                max_points: max_points as u32,
                evidence,
                feedback,
                next_step,
            }
        })
        .collect();

    Ok(Some(Scorecard {
        session_id: session_id.to_string(),
        rows: parsed_rows,
        grader_model: "coding-grader".to_string(),
    }))
}

/// Emits evidence events into `learning_evidence_events` for each scorecard row.
pub async fn emit_evidence_events(
    pool: &SqlitePool,
    program_id: &str,
    scorecard: &Scorecard,
    at: i64,
) -> Result<()> {
    for row in &scorecard.rows {
        let event_id = uuid::Uuid::new_v4().to_string();
        let result_str = match row.status {
            ScorecardStatus::Observed => "observed",
            ScorecardStatus::InsufficientEvidence => "not_assessed",
        };
        let score: Option<f64> =
            row.points.map(|p| (p as f64) / (row.max_points.max(1) as f64));
        let quote = row
            .evidence
            .iter()
            .find(|e| e.kind == EvidenceKind::Quote)
            .map(|e| e.text.clone());

        sqlx::query(
            "INSERT INTO learning_evidence_events (
                id, program_id, outcome_id, source_kind, source_id, dimension,
                result, score, observation, evidence_quote, assistance_json, observed_at
            ) VALUES (?, ?, NULL, 'simulation', ?, 'application', ?, ?, ?, ?, '[]', ?)",
        )
        .bind(&event_id)
        .bind(program_id)
        .bind(&scorecard.session_id)
        .bind(result_str)
        .bind(score)
        .bind(&row.feedback)
        .bind(quote)
        .bind(at)
        .execute(pool)
        .await
        .map_err(db_err)?;
    }

    Ok(())
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

        // Silence on Communication must be InsufficientEvidence with points: None
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
