//! Coding sessions: their rows, events, examples, cues, clock and debrief. Every statement the coding tables need is
//! here, so a run, a turn and a stage move share one pool, one writer lock and one operation ledger.
use super::{
    decode, invalid, json, practical_operation, uuid, InternalActivitySnapshot,
    LearningPracticalRepository,
};
use crate::lab_runtime::LearningLabFile;
use crate::operations::Operation;
use crate::persistence::db;
use crate::practical_dto::LearningPracticalActivityKind;
use crate::practice::coding::dto::{
    stages_of, ClockDto, CodeHintDto, CodingRunScope, CodingStage, ExampleVerdict,
};
use crate::practice::coding::examples::ExampleCheck;
use crate::practice::coding::policy::CodingCue;
use crate::practice::coding::problem::{CodingSetting, CodingTask, Hidden, Problem};
use crate::practice::coding::records::{
    Applied, CodingOp, CodingRow, NewCodingSession, NewHint, RunFinished, StageMove,
};
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
        uuid(program_id, "program")?;
        let mut tx = self.pool.begin_write().await.map_err(db)?;
        Self::writer_lock(&mut tx, program_id).await?;
        Ok(tx)
    }

    /// Whether this request was already answered, for a command whose slow work should not start twice.
    pub async fn coding_operation_seen(
        &self,
        program_id: &str,
        subject_id: &str,
        op: &CodingOp<'_>,
    ) -> Result<bool> {
        uuid(op.operation_id, "operation")?;
        op.operation(program_id, subject_id).seen(&self.pool).await
    }

    /// Creates the simulation session and the coding session on it. Returns the session's id and whether this call
    /// created it: a seen operation id answers with the session it made.
    pub async fn create_coding_session(
        &self,
        op: &CodingOp<'_>,
        new: &NewCodingSession<'_>,
        at: i64,
    ) -> Result<(String, bool)> {
        uuid(op.operation_id, "operation")?;
        uuid(new.activity_id, "activity")?;
        let mut tx = self.begin_coding_write(new.program_id).await?;
        let operation = op.operation(new.program_id, new.activity_id);
        if let Some(session_id) = operation.replay::<String, _>(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok((session_id, false));
        }
        Self::active_program(&mut tx, new.program_id).await?;
        let activity = Self::internal_activity(&mut tx, new.program_id, new.activity_id).await?;
        if activity.kind != LearningPracticalActivityKind::CodingProblem {
            return Err(invalid("This activity is not a coding problem."));
        }
        let problem = problem_of(&activity)?;
        let task = if problem.bug.is_some() {
            CodingTask::BugFix
        } else {
            CodingTask::Problem
        };
        let first = stages_of(task)
            .first()
            .copied()
            .ok_or_else(|| invalid("This kind of task has no stages yet."))?;
        let starter = activity
            .files
            .iter()
            .filter(|file| file.role == "starter" && file.editable)
            .map(|file| LearningLabFile {
                path: file.path.clone(),
                content: file.content.clone(),
            })
            .collect::<Vec<_>>();
        if starter.is_empty() {
            return Err(invalid("This coding problem has no starter file."));
        }
        let budget = match new.setting {
            CodingSetting::Coached => None,
            CodingSetting::Mock | CodingSetting::Watched => Some(
                new.budget_seconds
                    .unwrap_or_else(|| (new.default_budget_seconds)(problem.spec.difficulty)),
            ),
        };
        let session_id = uuid::Uuid::new_v4().to_string();
        let counterpart = match new.setting {
            CodingSetting::Coached => "Coach",
            CodingSetting::Mock | CodingSetting::Watched => "Interviewer",
        };
        sqlx::query("INSERT INTO learning_simulation_sessions(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,learner_role,counterpart_role,status,revision,created_at,updated_at,submitted_at) VALUES(?,?,?,?,?,NULL,?,?,'Candidate',?,'active',0,?,?,NULL)")
            .bind(&session_id).bind(new.program_id).bind(new.activity_id)
            .bind(activity.revision).bind(json(&activity)?)
            .bind(op.operation_id).bind(op.payload_hash).bind(counterpart)
            .bind(at).bind(at)
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_coding_sessions(id,program_id,activity_id,task,setting,stage,budget_seconds,active_ms,opened_at,draft_json,pending_cue_json,submitted_at,solution_seen_at,finished_at,revision,created_at) VALUES(?,?,?,?,?,?,?,0,NULL,?,NULL,NULL,NULL,NULL,0,?)")
            .bind(&session_id).bind(new.program_id).bind(new.activity_id)
            .bind(name_of(&task)?).bind(name_of(&new.setting)?).bind(first.as_str())
            .bind(budget.map(i64::from)).bind(json(&starter)?).bind(at)
            .execute(&mut *tx).await.map_err(db)?;
        add_event(
            &mut tx,
            &session_id,
            "stage",
            &serde_json::json!({"from": null, "to": first, "skipped": []}),
            at,
        )
        .await?;
        operation.record(&mut *tx, &session_id).await?;
        tx.commit().await.map_err(db)?;
        Ok((session_id, true))
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
        uuid(session_id, "coding session")?;
        uuid(op.operation_id, "operation")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let operation = op.operation(program_id, session_id);
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(Applied::Replayed);
        }
        let row = open_row_on(&mut tx, program_id, session_id).await?;
        check_revision(&row, expected_revision)?;
        let stages = stages_of(row.task);
        let position = |stage: CodingStage| stages.iter().position(|each| *each == stage);
        let target = position(to).ok_or_else(|| invalid("That stage is not part of this task."))?;
        let current = position(row.stage).ok_or_else(|| {
            AppError::Database("The session is in a stage its task lacks.".into())
        })?;
        let skipped = if target > current + 1 {
            stages
                .iter()
                .skip(current + 1)
                .take(target - current - 1)
                .copied()
                .collect()
        } else {
            Vec::new()
        };
        let moved = StageMove {
            from: row.stage,
            to,
            skipped,
        };
        if moved.from != moved.to {
            sqlx::query("UPDATE learning_coding_sessions SET stage=? WHERE id=?")
                .bind(to.as_str())
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            add_event(
                &mut tx,
                session_id,
                "stage",
                &serde_json::json!({"from": moved.from, "to": moved.to, "skipped": moved.skipped}),
                at,
            )
            .await?;
            bump_revision(&mut tx, session_id).await?;
        }
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        Ok(Applied::Done(moved))
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
        uuid(session_id, "coding session")?;
        uuid(op.operation_id, "operation")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let operation = op.operation(program_id, session_id);
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(Applied::Replayed);
        }
        open_row_on(&mut tx, program_id, session_id).await?;
        let ordinal = match ordinal {
            Some(ordinal) => {
                let exists: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM learning_coding_examples WHERE session_id=? AND ordinal=?",
                )
                .bind(session_id)
                .bind(i64::from(ordinal))
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
                if exists.is_none() {
                    return Err(AppError::NotFound("Example not found".into()));
                }
                ordinal
            }
            None => {
                let next: i64 = sqlx::query_scalar(
                    "SELECT coalesce(max(ordinal),0)+1 FROM learning_coding_examples WHERE session_id=?",
                )
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
                to_u32(next)?
            }
        };
        sqlx::query("INSERT OR REPLACE INTO learning_coding_examples(session_id,ordinal,args_json,expected,verdict,reference_actual,why_invalid,catches_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(session_id).bind(i64::from(ordinal)).bind(json(args)?).bind(expected)
            .bind(check.verdict.as_str()).bind(&check.reference_actual).bind(&check.why_invalid)
            .bind(json(&check.catches)?).bind(at)
            .execute(&mut *tx).await.map_err(db)?;
        add_event(
            &mut tx,
            session_id,
            "example",
            &serde_json::json!({"ordinal": ordinal, "verdict": check.verdict}),
            at,
        )
        .await?;
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        Ok(Applied::Done(ordinal))
    }

    pub async fn remove_coding_example(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        ordinal: u32,
        at: i64,
    ) -> Result<Applied<()>> {
        uuid(session_id, "coding session")?;
        uuid(op.operation_id, "operation")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let operation = op.operation(program_id, session_id);
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(Applied::Replayed);
        }
        open_row_on(&mut tx, program_id, session_id).await?;
        let removed =
            sqlx::query("DELETE FROM learning_coding_examples WHERE session_id=? AND ordinal=?")
                .bind(session_id)
                .bind(i64::from(ordinal))
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        if removed.rows_affected() == 1 {
            add_event(
                &mut tx,
                session_id,
                "example",
                &serde_json::json!({"ordinal": ordinal, "removed": true}),
                at,
            )
            .await?;
        }
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        Ok(Applied::Done(()))
    }

    /// Saves the draft the screen holds. A finished session keeps the draft it finished with.
    pub async fn save_coding_draft(
        &self,
        program_id: &str,
        session_id: &str,
        files: &[LearningLabFile],
    ) -> Result<()> {
        uuid(session_id, "coding session")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        open_row_on(&mut tx, program_id, session_id).await?;
        sqlx::query("UPDATE learning_coding_sessions SET draft_json=? WHERE id=?")
            .bind(json(files)?)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
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
        uuid(session_id, "coding session")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        open_row_on(&mut tx, program_id, session_id).await?;
        let recorded: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM learning_coding_events WHERE session_id=? AND kind='run' AND json_extract(payload_json,'$.runId')=?",
        )
        .bind(session_id)
        .bind(finished.run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let fresh = recorded.is_none();
        if fresh {
            let mut payload = serde_json::json!({
                "runId": finished.run_id,
                "scope": finished.scope,
                "passed": finished.passed,
                "total": finished.total,
            });
            if let (Some(actual), Some(map)) = (finished.reference_actual, payload.as_object_mut())
            {
                map.insert("referenceActual".into(), actual.into());
            }
            add_event(&mut tx, session_id, "run", &payload, at).await?;
            if finished.submit {
                sqlx::query("UPDATE learning_coding_sessions SET submitted_at=coalesce(submitted_at,?) WHERE id=?")
                    .bind(at)
                    .bind(session_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
            }
        }
        tx.commit().await.map_err(db)?;
        Ok(fresh)
    }

    /// Offers a cue for the counterpart. It replaces a pending one only when it outranks it. Returns whether a cue
    /// is pending afterwards.
    pub async fn offer_coding_cue(
        &self,
        program_id: &str,
        session_id: &str,
        cue: &CodingCue,
    ) -> Result<bool> {
        uuid(session_id, "coding session")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let row = row_on(&mut tx, program_id, session_id).await?;
        let replaces = row.finished_at.is_none()
            && row
                .pending_cue
                .as_ref()
                .map_or(true, |pending| cue.outranks(pending));
        if replaces {
            sqlx::query("UPDATE learning_coding_sessions SET pending_cue_json=? WHERE id=?")
                .bind(json(cue)?)
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(replaces || row.pending_cue.is_some())
    }

    /// Takes the pending cue: it is cleared in the same statement that returns it, so two callers never answer it
    /// twice.
    pub async fn take_coding_cue(
        &self,
        program_id: &str,
        session_id: &str,
    ) -> Result<Option<CodingCue>> {
        uuid(session_id, "coding session")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let taken: Option<String> = sqlx::query_scalar(
            "SELECT pending_cue_json FROM learning_coding_sessions WHERE id=? AND program_id=?",
        )
        .bind(session_id)
        .bind(program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .flatten();
        if taken.is_some() {
            sqlx::query(
                "UPDATE learning_coding_sessions SET pending_cue_json=NULL WHERE id=? AND program_id=?",
            )
            .bind(session_id)
            .bind(program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        taken.as_deref().map(decode).transpose()
    }

    /// Puts a taken cue back when the reply failed, unless a newer cue has arrived since.
    pub async fn restore_coding_cue(
        &self,
        program_id: &str,
        session_id: &str,
        cue: &CodingCue,
    ) -> Result<()> {
        uuid(session_id, "coding session")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        sqlx::query("UPDATE learning_coding_sessions SET pending_cue_json=? WHERE id=? AND program_id=? AND pending_cue_json IS NULL AND finished_at IS NULL")
            .bind(json(cue)?)
            .bind(session_id)
            .bind(program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
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
        uuid(session_id, "coding session")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let row = open_row_on(&mut tx, program_id, session_id).await?;
        if let Some(content) = reply {
            let speaker = match row.setting {
                CodingSetting::Coached => "coach",
                CodingSetting::Mock | CodingSetting::Watched => "counterpart",
            };
            add_turn(
                &mut tx,
                program_id,
                session_id,
                speaker,
                content,
                Some(model),
                at,
            )
            .await?;
        }
        add_event(
            &mut tx,
            session_id,
            "draft_seen",
            &serde_json::json!({"files": seen}),
            at,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(())
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
        uuid(session_id, "coding session")?;
        uuid(op.operation_id, "operation")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let operation = op.operation(program_id, session_id);
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(Applied::Replayed);
        }
        let row = open_row_on(&mut tx, program_id, session_id).await?;
        check_revision(&row, expected_revision)?;
        add_turn(
            &mut tx, program_id, session_id, "learner", content, None, at,
        )
        .await?;
        sqlx::query("UPDATE learning_coding_sessions SET draft_json=? WHERE id=?")
            .bind(json(files)?)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        bump_revision(&mut tx, session_id).await?;
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        Ok(Applied::Done(()))
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
        uuid(session_id, "coding session")?;
        uuid(op.operation_id, "operation")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let operation = op.operation(program_id, session_id);
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(Applied::Replayed);
        }
        let row = open_row_on(&mut tx, program_id, session_id).await?;
        check_revision(&row, expected_revision)?;
        let given: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM learning_coding_events WHERE session_id=? AND kind='hint'",
        )
        .bind(session_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let run_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM learning_practical_runs WHERE coding_session_id=? ORDER BY created_at DESC,id DESC LIMIT 1",
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let stored = CodeHintDto {
            ordinal: to_u32(given + 1)?,
            rung: hint.rung,
            text: hint.text.to_owned(),
            reason: hint.reason.to_owned(),
            lines: hint.lines.to_vec(),
            path: hint.path.map(str::to_owned),
            stage: row.stage,
            run_id,
            in_role: hint.in_role,
            at,
        };
        add_event(
            &mut tx,
            session_id,
            "hint",
            &serde_json::to_value(&stored)
                .map_err(|error| AppError::Serialization(error.to_string()))?,
            at,
        )
        .await?;
        if hint.in_role {
            add_turn(
                &mut tx,
                program_id,
                session_id,
                "counterpart",
                hint.text,
                Some(hint.model),
                at,
            )
            .await?;
        }
        sqlx::query("UPDATE learning_coding_sessions SET draft_json=? WHERE id=?")
            .bind(json(files)?)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        bump_revision(&mut tx, session_id).await?;
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        Ok(Applied::Done(stored))
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
        uuid(session_id, "coding session")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let row = row_on(&mut tx, program_id, session_id).await?;
        let mut active_ms = row.active_ms;
        let mut opened_at = row.opened_at;
        if row.finished_at.is_none() {
            if let Some(opened) = row.opened_at {
                let last_event: Option<i64> = sqlx::query_scalar(
                    "SELECT max(at) FROM learning_coding_events WHERE session_id=?",
                )
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
                let ends = if open {
                    last_event.unwrap_or(opened).max(opened)
                } else {
                    at.max(opened)
                };
                active_ms += u64::try_from(ends - opened).unwrap_or(0);
                add_event(&mut tx, session_id, "closed", &serde_json::json!({}), ends).await?;
                opened_at = None;
            }
            if open {
                add_event(&mut tx, session_id, "opened", &serde_json::json!({}), at).await?;
                opened_at = Some(at);
            }
            sqlx::query("UPDATE learning_coding_sessions SET active_ms=?,opened_at=? WHERE id=?")
                .bind(i64::try_from(active_ms).unwrap_or(i64::MAX))
                .bind(opened_at)
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(ClockDto {
            active_ms,
            open_since: opened_at,
            budget_seconds: row.budget_seconds,
        })
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
        uuid(session_id, "coding session")?;
        uuid(op.operation_id, "operation")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let operation = op.operation(program_id, session_id);
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(Applied::Replayed);
        }
        let row = open_row_on(&mut tx, program_id, session_id).await?;
        check_revision(&row, expected_revision)?;
        if row.setting != setting {
            let budget = match (setting, row.budget_seconds) {
                (CodingSetting::Coached, budget) => budget,
                (_, None) => Some(default_budget_seconds),
                (_, budget) => budget,
            };
            sqlx::query(
                "UPDATE learning_coding_sessions SET setting=?,budget_seconds=? WHERE id=?",
            )
            .bind(name_of(&setting)?)
            .bind(budget.map(i64::from))
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            let counterpart = match setting {
                CodingSetting::Coached => "Coach",
                CodingSetting::Mock | CodingSetting::Watched => "Interviewer",
            };
            sqlx::query("UPDATE learning_simulation_sessions SET counterpart_role=? WHERE id=?")
                .bind(counterpart)
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            add_event(
                &mut tx,
                session_id,
                "setting",
                &serde_json::json!({"from": row.setting, "to": setting}),
                at,
            )
            .await?;
            bump_revision(&mut tx, session_id).await?;
        }
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        Ok(Applied::Done(()))
    }

    /// Marks the worked solution as seen, when the setting lets the learner see it yet.
    pub async fn reveal_coding_solution(
        &self,
        program_id: &str,
        session_id: &str,
        op: &CodingOp<'_>,
        at: i64,
    ) -> Result<Applied<()>> {
        uuid(session_id, "coding session")?;
        uuid(op.operation_id, "operation")?;
        let mut tx = self.begin_coding_write(program_id).await?;
        let operation = op.operation(program_id, session_id);
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(Applied::Replayed);
        }
        let row = open_row_on(&mut tx, program_id, session_id).await?;
        if !row
            .setting
            .discloses(Hidden::WorkedSolution, row.submitted_at.is_some())
        {
            return Err(invalid(
                "The worked solution is not available in this setting yet.",
            ));
        }
        if row.solution_seen_at.is_none() {
            sqlx::query("UPDATE learning_coding_sessions SET solution_seen_at=? WHERE id=?")
                .bind(at)
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            add_event(
                &mut tx,
                session_id,
                "solution_seen",
                &serde_json::json!({}),
                at,
            )
            .await?;
            bump_revision(&mut tx, session_id).await?;
        }
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        Ok(Applied::Done(()))
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
