//! Reading coding sessions: one in full, the list, a run's stored detail, and what the Practice page counts as due.
use super::super::{
    decode, parse_run_status, parse_speaker, uuid, InternalActivitySnapshot,
    LearningPracticalRepository,
};
use super::{from_name, parse_row, problem_of, row_on, scope_of, to_u32, verdict_of};
use crate::persistence::db;
use crate::practical_dto::{CaseCheckDetailDto, LearningSimulationTurnDto};
use crate::practice::coding::dto::{CodingSessionSummaryDto, PatternRatingDto, PracticeDueDto};
use crate::practice::coding::patterns;
use crate::practice::coding::records::{
    SessionBundle, StoredDebrief, StoredEvent, StoredExample, StoredRun,
};
use lattice_core::domain::learning::practice::{Scorecard, ScorecardRow};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Transactions;
use sqlx::{Row, SqliteConnection};

async fn events_on(
    connection: &mut SqliteConnection,
    session_id: &str,
) -> Result<Vec<StoredEvent>> {
    sqlx::query("SELECT ordinal,kind,payload_json,at FROM learning_coding_events WHERE session_id=? ORDER BY ordinal")
        .bind(session_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
        .iter()
        .map(|row| {
            Ok(StoredEvent {
                ordinal: to_u32(row.get("ordinal"))?,
                kind: row.get("kind"),
                payload: decode(row.get("payload_json"))?,
                at: row.get("at"),
            })
        })
        .collect()
}

async fn examples_on(
    connection: &mut SqliteConnection,
    session_id: &str,
) -> Result<Vec<StoredExample>> {
    sqlx::query("SELECT * FROM learning_coding_examples WHERE session_id=? ORDER BY ordinal")
        .bind(session_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
        .iter()
        .map(|row| {
            Ok(StoredExample {
                ordinal: to_u32(row.get("ordinal"))?,
                args: decode(row.get("args_json"))?,
                expected: row.get("expected"),
                verdict: verdict_of(row.get("verdict"))?,
                reference_actual: row.get("reference_actual"),
                why_invalid: row.get("why_invalid"),
                catches: decode(row.get("catches_json"))?,
            })
        })
        .collect()
}

async fn runs_on(
    connection: &mut SqliteConnection,
    session_id: &str,
    only: Option<&str>,
) -> Result<Vec<StoredRun>> {
    let rows = sqlx::query("SELECT * FROM learning_practical_runs WHERE coding_session_id=?1 AND (?2 IS NULL OR id=?2) ORDER BY created_at,id")
        .bind(session_id)
        .bind(only)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
    let mut runs = Vec::new();
    for row in rows {
        let run_id: String = row.get("id");
        let details = sqlx::query_scalar::<_, Option<String>>(
            "SELECT detail_json FROM learning_practical_run_checks WHERE run_id=? ORDER BY ordinal",
        )
        .bind(&run_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
        let cases = details
            .iter()
            .flatten()
            .map(|detail| decode::<CaseCheckDetailDto>(detail))
            .collect::<Result<Vec<_>>>()?;
        let scope: Option<String> = row.get("scope");
        runs.push(StoredRun {
            run_id,
            scope: scope_of(
                scope
                    .as_deref()
                    .ok_or_else(|| AppError::Database("A coding run has no scope.".into()))?,
            )?,
            status: parse_run_status(row.get("status"))?,
            stdout: row.get("stdout"),
            stderr: row.get("stderr"),
            duration_ms: row.get("duration_ms"),
            files: decode(row.get("learner_files_json"))?,
            created_at: row.get("created_at"),
            cases,
        });
    }
    Ok(runs)
}

async fn turns_on(
    connection: &mut SqliteConnection,
    session_id: &str,
) -> Result<Vec<LearningSimulationTurnDto>> {
    sqlx::query("SELECT * FROM learning_simulation_turns WHERE session_id=? ORDER BY ordinal")
        .bind(session_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
        .iter()
        .map(|turn| {
            Ok(LearningSimulationTurnDto {
                id: turn.get("id"),
                ordinal: turn.get("ordinal"),
                speaker: parse_speaker(turn.get("speaker"))?,
                content: turn.get("content"),
                citations: decode(turn.get("citations_json"))?,
                model_name: turn.get("model_name"),
                created_at: turn.get("created_at"),
            })
        })
        .collect()
}

fn points_of(value: i64) -> Result<u8> {
    u8::try_from(value).map_err(|_| AppError::Database("Stored points are out of range.".into()))
}

async fn scorecard_on(
    connection: &mut SqliteConnection,
    session_id: &str,
) -> Result<Option<Scorecard>> {
    let rows = sqlx::query("SELECT * FROM learning_scorecards WHERE session_id=? ORDER BY ordinal")
        .bind(session_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
    if rows.is_empty() {
        return Ok(None);
    }
    let rows = rows
        .iter()
        .map(|row| {
            Ok(ScorecardRow {
                criterion_id: row.get("criterion_id"),
                title: row.get("title"),
                status: from_name(row.get("status"))?,
                points: row
                    .get::<Option<i64>, _>("points")
                    .map(points_of)
                    .transpose()?,
                max_points: points_of(row.get("max_points"))?,
                evidence: decode(row.get("evidence_json"))?,
                feedback: row.get("feedback"),
                next_step: row.get("next_step"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(Scorecard { rows }))
}

async fn debrief_on(
    connection: &mut SqliteConnection,
    session_id: &str,
) -> Result<Option<StoredDebrief>> {
    sqlx::query("SELECT approach,complexity,ratings_json,model FROM learning_coding_debriefs WHERE session_id=?")
        .bind(session_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?
        .map(|row| {
            Ok(StoredDebrief {
                approach: row.get("approach"),
                complexity: row.get("complexity"),
                ratings: decode::<Vec<PatternRatingDto>>(row.get("ratings_json"))?,
                model: row.get("model"),
            })
        })
        .transpose()
}

impl LearningPracticalRepository {
    /// Everything stored about one session, read in one transaction.
    pub async fn coding_session(
        &self,
        program_id: &str,
        session_id: &str,
    ) -> Result<SessionBundle> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// A run of the session with the full result of each case.
    pub async fn coding_run(
        &self,
        program_id: &str,
        session_id: &str,
        run_id: &str,
    ) -> Result<StoredRun> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// The program's coding sessions, newest first.
    pub async fn list_coding_sessions(
        &self,
        program_id: &str,
    ) -> Result<Vec<crate::practice::coding::dto::CodingSessionSummaryDto>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Whether anything but the draft changed after `since`: a run, a stage move or a learner turn.
    pub async fn coding_worked_since(&self, session_id: &str, since: i64) -> Result<bool> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// What the Practice page counts as due: coding patterns and the drills'.
    pub async fn practice_due(&self, program_id: &str, now: i64) -> Result<PracticeDueDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }
}
