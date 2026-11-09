//! Coding sessions: their rows, events, examples, cues, clock and debrief. Every statement the coding tables need is
//! here, so a run, a turn and a stage move share one pool, one writer lock and one operation ledger.
use super::{
    decode, invalid, json, practical_operation, uuid, InternalActivitySnapshot,
    LearningPracticalRepository,
};
use crate::operations::Operation;
use crate::persistence::db;
use crate::practice::coding::dto::{
    stages_of, ClockDto, CodeHintDto, CodingRunScope, CodingStage, ExampleVerdict,
};
use crate::practice::coding::examples::ExampleCheck;
use crate::practice::coding::policy::CodingCue;
use crate::practice::coding::problem::{CodingSetting, CodingTask, Hidden, Problem};
use crate::practice::coding::records::{
    Applied, CodingOp, CodingRow, NewCodingSession, NewHint, RunFinished, StageMove,
};
use crate::practical_dto::LearningPracticalActivityKind;
use crate::lab_runtime::LearningLabFile;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Transactions;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqliteConnection, Transaction};

mod finish;
mod reads;

/// Where a coding activity keeps its hidden problem: a `check` file, never a `reference` one, so no public read
/// returns it.
pub const PROBLEM_FILE: &str = "problem/problem.json";

/// The name a plain enum is stored under.
fn name_of<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => Ok(name),
        _ => Err(AppError::Serialization("Expected a plain name.".into())),
    }
}

fn from_name<T: DeserializeOwned>(name: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(name.to_owned()))
        .map_err(|error| AppError::Serialization(error.to_string()))
}

fn to_u32(value: i64) -> Result<u32> {
    u32::try_from(value).map_err(|_| AppError::Database("A stored count is out of range.".into()))
}

impl CodingOp<'_> {
    fn operation<'b>(&'b self, program_id: &'b str, subject_id: &'b str) -> Operation<'b> {
        practical_operation(
            "coding",
            self.operation_id,
            Some(program_id),
            subject_id,
            self.kind,
            self.payload_hash,
            "Operation ID was reused with different coding session data.",
        )
    }
}

pub(super) fn parse_row(row: &SqliteRow) -> Result<CodingRow> {
    Ok(CodingRow {
        id: row.get("id"),
        program_id: row.get("program_id"),
        activity_id: row.get("activity_id"),
        task: from_name(row.get("task"))?,
        setting: from_name(row.get("setting"))?,
        stage: CodingStage::parse(row.get("stage"))
            .ok_or_else(|| AppError::Database("Invalid coding stage".into()))?,
        budget_seconds: row
            .get::<Option<i64>, _>("budget_seconds")
            .map(|seconds| to_u32(seconds))
            .transpose()?,
        active_ms: u64::try_from(row.get::<i64, _>("active_ms"))
            .map_err(|_| AppError::Database("A stored duration is negative.".into()))?,
        opened_at: row.get("opened_at"),
        draft: decode(row.get("draft_json"))?,
        pending_cue: row
            .get::<Option<String>, _>("pending_cue_json")
            .as_deref()
            .map(decode)
            .transpose()?,
        submitted_at: row.get("submitted_at"),
        solution_seen_at: row.get("solution_seen_at"),
        finished_at: row.get("finished_at"),
        revision: row.get("revision"),
        created_at: row.get("created_at"),
    })
}

/// The session's row, or NotFound.
pub(super) async fn row_on(
    connection: &mut SqliteConnection,
    program_id: &str,
    session_id: &str,
) -> Result<CodingRow> {
    let row = sqlx::query("SELECT * FROM learning_coding_sessions WHERE id=? AND program_id=?")
        .bind(session_id)
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Coding session not found".into()))?;
    parse_row(&row)
}

/// The row of a session that still takes changes.
async fn open_row_on(
    connection: &mut SqliteConnection,
    program_id: &str,
    session_id: &str,
) -> Result<CodingRow> {
    let row = row_on(connection, program_id, session_id).await?;
    if row.finished_at.is_some() {
        return Err(invalid("This coding session is finished."));
    }
    Ok(row)
}

fn check_revision(row: &CodingRow, expected: i64) -> Result<()> {
    if row.revision == expected {
        Ok(())
    } else {
        Err(AppError::Conflict(
            "The coding session changed; reload and retry.".into(),
        ))
    }
}

/// Adds an event after the session's last one and returns its ordinal.
pub(super) async fn add_event(
    connection: &mut SqliteConnection,
    session_id: &str,
    kind: &str,
    payload: &serde_json::Value,
    at: i64,
) -> Result<u32> {
    let ordinal: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(ordinal),-1)+1 FROM learning_coding_events WHERE session_id=?",
    )
    .bind(session_id)
    .fetch_one(&mut *connection)
    .await
    .map_err(db)?;
    sqlx::query("INSERT INTO learning_coding_events(session_id,ordinal,kind,payload_json,at) VALUES(?,?,?,?,?)")
        .bind(session_id)
        .bind(ordinal)
        .bind(kind)
        .bind(json(payload)?)
        .bind(at)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    to_u32(ordinal)
}

async fn bump_revision(connection: &mut SqliteConnection, session_id: &str) -> Result<()> {
    sqlx::query("UPDATE learning_coding_sessions SET revision=revision+1 WHERE id=?")
        .bind(session_id)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Appends a turn to the session's transcript.
pub(super) async fn add_turn(
    connection: &mut SqliteConnection,
    program_id: &str,
    session_id: &str,
    speaker: &str,
    content: &str,
    model: Option<&str>,
    at: i64,
) -> Result<()> {
    let ordinal: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(ordinal),-1)+1 FROM learning_simulation_turns WHERE session_id=?",
    )
    .bind(session_id)
    .fetch_one(&mut *connection)
    .await
    .map_err(db)?;
    sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,?,?,?,'[]',?,?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(program_id)
        .bind(session_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(crate::persistence::hash_text(content))
        .bind(ordinal)
        .bind(speaker)
        .bind(content)
        .bind(model)
        .bind(at)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

fn problem_of(activity: &InternalActivitySnapshot) -> Result<Problem> {
    let file = activity
        .files
        .iter()
        .find(|file| file.path == PROBLEM_FILE && file.role == "check")
        .ok_or_else(|| invalid("This activity holds no coding problem."))?;
    decode(&file.content)
}

impl LearningPracticalRepository {
    async fn begin_coding_write(&self, program_id: &str) -> Result<Transaction<'static, Sqlite>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Whether this request was already answered, for a command whose slow work should not start twice.
    pub async fn coding_operation_seen(
        &self,
        program_id: &str,
        subject_id: &str,
        op: &CodingOp<'_>,
    ) -> Result<bool> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Creates the simulation session and the coding session on it. Returns the session's id and whether this call
    /// created it: a seen operation id answers with the session it made.
    pub async fn create_coding_session(
        &self,
        op: &CodingOp<'_>,
        new: &NewCodingSession<'_>,
        at: i64,
    ) -> Result<(String, bool)> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Moves to `to`, recording the stages passed over.
    pub async fn move_coding_stage(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        to: CodingStage,
        expected_revision: i64,
        at: i64,
    ) -> Result<Applied<StageMove>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Stores a learner's example with the reference's verdict: a new row, or the row `ordinal` names.
    pub async fn save_coding_example(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        ordinal: Option<u32>,
        args: &[String],
        expected: &str,
        check: &ExampleCheck,
        at: i64,
    ) -> Result<Applied<u32>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    pub async fn remove_coding_example(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        ordinal: u32,
        at: i64,
    ) -> Result<Applied<()>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Saves the draft the screen holds. A finished session keeps the draft it finished with.
    pub async fn save_coding_draft(
        &self,
        program_id: &str,
        session_id: &str,
        files: &[LearningLabFile],
    ) -> Result<()> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Records a finished run once: the event and, for a submit, when the learner first submitted. Returns whether
    /// this call recorded it; a run already recorded changes nothing.
    pub async fn record_coding_run(
        &self,
        program_id: &str,
        session_id: &str,
        finished: &RunFinished<'_>,
        at: i64,
    ) -> Result<bool> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Offers a cue for the counterpart. It replaces a pending one only when it outranks it. Returns whether a cue
    /// is pending afterwards.
    pub async fn offer_coding_cue(
        &self,
        program_id: &str,
        session_id: &str,
        cue: &CodingCue,
    ) -> Result<bool> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Takes the pending cue: it is cleared in the same statement that returns it, so two callers never answer it
    /// twice.
    pub async fn take_coding_cue(
        &self,
        program_id: &str,
        session_id: &str,
    ) -> Result<Option<CodingCue>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Puts a taken cue back when the reply failed, unless a newer cue has arrived since.
    pub async fn restore_coding_cue(
        &self,
        program_id: &str,
        session_id: &str,
        cue: &CodingCue,
    ) -> Result<()> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Stores a reply as the counterpart's turn, and the draft it was written from as the last one it saw. A
    /// `None` reply is silence: only the draft is recorded.
    pub async fn record_coding_reply(
        &self,
        program_id: &str,
        session_id: &str,
        reply: Option<&str>,
        model: &str,
        seen: &[LearningLabFile],
        at: i64,
    ) -> Result<()> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Stores the learner's turn with the draft beside it.
    pub async fn append_coding_turn(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        content: &str,
        files: &[LearningLabFile],
        expected_revision: i64,
        at: i64,
    ) -> Result<Applied<()>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Stores a hint the writer produced, numbered after the hints already given.
    pub async fn record_coding_hint(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        hint: &NewHint<'_>,
        files: &[LearningLabFile],
        expected_revision: i64,
        at: i64,
    ) -> Result<Applied<CodeHintDto>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Opens or closes the clock. Opening over a span the app never closed ends that span at the session's last
    /// event, so a crash never counts the hours until the next start.
    pub async fn set_coding_clock(
        &self,
        program_id: &str,
        session_id: &str,
        open: bool,
        at: i64,
    ) -> Result<ClockDto> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Changes the setting. Leaving Coached for another setting with no budget gives the session `default_budget`.
    pub async fn change_coding_setting(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        setting: CodingSetting,
        default_budget_seconds: u32,
        expected_revision: i64,
        at: i64,
    ) -> Result<Applied<()>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }

    /// Marks the worked solution as seen, when the setting lets the learner see it yet.
    pub async fn reveal_coding_solution(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        at: i64,
    ) -> Result<Applied<()>> {
        Err(AppError::InvalidState("Not built yet.".into()))
    }
}

/// How an example's verdict is stored and read back.
pub(super) fn verdict_of(name: &str) -> Result<ExampleVerdict> {
    ExampleVerdict::parse(name).ok_or_else(|| AppError::Database("Invalid example verdict".into()))
}

/// How a run's scope is stored and read back.
pub(super) fn scope_of(name: &str) -> Result<CodingRunScope> {
    CodingRunScope::parse(name).ok_or_else(|| AppError::Database("Invalid run scope".into()))
}
