//! Where audit runs are kept (lesson claim review §14.7):
//! `learning_lesson_audits`, one row per lesson, draft and Verifier model.
//! Each batch's record and token usage is written before the next batch is
//! sent, so a stopped or interrupted audit shows how far it got and what it
//! cost. Which claims still need auditing is the ledger's answer (their
//! receipts), not this table's.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::{AppError, Result};

use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::features::learning::persistence::{db, decode, encode};

use super::policy::consent;
use super::{AuditGate, AuditStop};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BatchOutcome {
    Done,
    Failed,
}

/// One batch as recorded: which claims it carried, what it cost and how it
/// ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchRecord {
    /// 1-based within the pass that sent it, of `of` planned.
    pub index: usize,
    pub of: usize,
    pub claim_ids: Vec<String>,
    pub outcome: BatchOutcome,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// The provider's error, sanitized, for a failed batch.
    pub error: Option<String>,
    /// The failure's kind (`AppError::error_code`): what two failures are
    /// compared by, since an error's words may carry an id that changes.
    pub error_code: Option<String>,
    /// Why web search could not be used in this batch.
    pub web_note: Option<String>,
    pub at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunState {
    Running,
    Completed,
    Stopped,
}

impl RunState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
        }
    }

    fn from_column(value: &str) -> Result<Self> {
        [Self::Running, Self::Completed, Self::Stopped]
            .into_iter()
            .find(|state| state.as_str() == value)
            .ok_or_else(|| AppError::Database(format!("unknown audit state {value}")))
    }
}

/// The row an audit writes: one lesson draft audited by one Verifier model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunKey {
    pub lesson_id: String,
    pub draft_sha256: String,
    pub verifier_model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditRun {
    pub key: RunKey,
    pub policy: String,
    pub state: RunState,
    pub batches: Vec<BatchRecord>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

impl AuditRun {
    /// The kind of error the last batch failed with, when the last batch
    /// failed: no batch has completed since.
    pub(crate) fn last_failure(&self) -> Option<&str> {
        self.batches
            .last()
            .filter(|batch| batch.outcome == BatchOutcome::Failed)
            .and_then(|batch| batch.error_code.as_deref())
    }
}

#[async_trait]
pub(crate) trait AuditRunStore: Send + Sync {
    /// Opens the run for `key` (or reopens it, keeping its batches) as running.
    async fn begin(&self, key: &RunKey, policy: &str, at: i64) -> Result<AuditRun>;
    /// Appends one batch's record and adds its tokens to the run's totals.
    async fn record(&self, key: &RunKey, batch: &BatchRecord) -> Result<()>;
    async fn finish(&self, key: &RunKey, state: RunState, at: i64) -> Result<()>;
}

/// The audit tokens a course has spent, for its Plan tab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CourseAuditUsage {
    pub audits: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone)]
pub(crate) struct SqliteAuditRuns {
    pool: SqlitePool,
}

impl SqliteAuditRuns {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn load(&self, key: &RunKey) -> Result<Option<AuditRun>> {
        sqlx::query(
            "SELECT * FROM learning_lesson_audits \
             WHERE lesson_id=? AND draft_sha256=? AND verifier_model=?",
        )
        .bind(&key.lesson_id)
        .bind(&key.draft_sha256)
        .bind(&key.verifier_model)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .map(|row| run_from_row(&row))
        .transpose()
    }

    /// The lesson's most recently started audit.
    pub(crate) async fn latest(&self, lesson_id: &str) -> Result<Option<AuditRun>> {
        sqlx::query(
            "SELECT * FROM learning_lesson_audits WHERE lesson_id=? \
             ORDER BY started_at DESC, rowid DESC LIMIT 1",
        )
        .bind(lesson_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .map(|row| run_from_row(&row))
        .transpose()
    }

    pub(crate) async fn course_usage(&self, program_id: &str) -> Result<CourseAuditUsage> {
        let row = sqlx::query(
            "SELECT count(*) AS audits, coalesce(sum(a.input_tokens),0) AS input_tokens, \
             coalesce(sum(a.output_tokens),0) AS output_tokens \
             FROM learning_lesson_audits a \
             JOIN learning_lesson_reviews r ON r.lesson_id=a.lesson_id WHERE r.program_id=?",
        )
        .bind(program_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        let count = |column: &str| u64::try_from(row.get::<i64, _>(column)).unwrap_or_default();
        Ok(CourseAuditUsage {
            audits: count("audits"),
            input_tokens: count("input_tokens"),
            output_tokens: count("output_tokens"),
        })
    }
}

fn run_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<AuditRun> {
    let tokens = |column: &str| u64::try_from(row.get::<i64, _>(column)).unwrap_or_default();
    Ok(AuditRun {
        key: RunKey {
            lesson_id: row.get("lesson_id"),
            draft_sha256: row.get("draft_sha256"),
            verifier_model: row.get("verifier_model"),
        },
        policy: row.get("policy"),
        state: RunState::from_column(row.get("state"))?,
        batches: decode(row.get("batches_json"))?,
        input_tokens: tokens("input_tokens"),
        output_tokens: tokens("output_tokens"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
    })
}

fn signed(tokens: u64) -> i64 {
    i64::try_from(tokens).unwrap_or(i64::MAX)
}

#[async_trait]
impl AuditRunStore for SqliteAuditRuns {
    async fn begin(&self, key: &RunKey, policy: &str, at: i64) -> Result<AuditRun> {
        sqlx::query(
            "INSERT INTO learning_lesson_audits \
             (lesson_id, draft_sha256, verifier_model, policy, state, batches_json, started_at) \
             VALUES (?, ?, ?, ?, 'running', '[]', ?) \
             ON CONFLICT(lesson_id, draft_sha256, verifier_model) \
             DO UPDATE SET state='running', policy=excluded.policy, finished_at=NULL",
        )
        .bind(&key.lesson_id)
        .bind(&key.draft_sha256)
        .bind(&key.verifier_model)
        .bind(policy)
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        self.load(key)
            .await?
            .ok_or_else(|| AppError::Database("the audit row was not written".into()))
    }

    async fn record(&self, key: &RunKey, batch: &BatchRecord) -> Result<()> {
        let written = sqlx::query(
            "UPDATE learning_lesson_audits \
             SET batches_json=json_insert(batches_json, '$[#]', json(?)), \
                 input_tokens=input_tokens+?, output_tokens=output_tokens+? \
             WHERE lesson_id=? AND draft_sha256=? AND verifier_model=?",
        )
        .bind(encode(batch)?)
        .bind(signed(batch.input_tokens))
        .bind(signed(batch.output_tokens))
        .bind(&key.lesson_id)
        .bind(&key.draft_sha256)
        .bind(&key.verifier_model)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if written.rows_affected() == 0 {
            return Err(AppError::NotFound("The lesson's audit is gone".into()));
        }
        Ok(())
    }

    async fn finish(&self, key: &RunKey, state: RunState, at: i64) -> Result<()> {
        sqlx::query(
            "UPDATE learning_lesson_audits SET state=?, finished_at=? \
             WHERE lesson_id=? AND draft_sha256=? AND verifier_model=?",
        )
        .bind(state.as_str())
        .bind(at)
        .bind(&key.lesson_id)
        .bind(&key.draft_sha256)
        .bind(&key.verifier_model)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}

/// The course's opt-in and consent, re-read at every batch boundary, held
/// against the model the audit pinned.
pub(crate) struct ProgramGate {
    pub programs: LearningCurriculumRepository,
    pub program_id: String,
    pub model: ModelRef,
}

#[async_trait]
impl AuditGate for ProgramGate {
    async fn check(&self) -> Result<Option<AuditStop>> {
        let setting = self.programs.verifier_audit(&self.program_id).await?;
        if !setting.on {
            return Ok(Some(AuditStop::TurnedOff));
        }
        Ok(AuditStop::from_consent(&consent(
            &self.model,
            setting.consent_provider.as_deref(),
        )))
    }
}

#[cfg(test)]
#[path = "runs_tests.rs"]
mod tests;
