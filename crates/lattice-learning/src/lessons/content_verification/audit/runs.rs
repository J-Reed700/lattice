//! Where audit runs are kept (lesson claim review §14.7):
//! `learning_lesson_audits`, one row per lesson, draft and Verifier model.
//! Each batch's record and token usage is written before the next batch is
//! sent, so a stopped or interrupted audit shows how far it got and what it
//! cost. Which claims still need auditing is the ledger's answer (their
//! receipts), not this table's.

use async_trait::async_trait;
use lattice_core::shared::persistence::{Database, DatabaseHandle};
use serde::{Deserialize, Serialize};

use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::{AppError, Result};

use crate::curriculum_repository::LearningCurriculumRepository;

use super::policy::consent;
use super::repository;
use super::{AuditGate, AuditStop};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchOutcome {
    Done,
    Failed,
}

/// One batch as recorded: which claims it carried, what it cost and how it
/// ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRecord {
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
pub enum RunState {
    Running,
    Completed,
    Stopped,
}

impl RunState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
        }
    }

    pub(super) fn from_column(value: &str) -> Result<Self> {
        [Self::Running, Self::Completed, Self::Stopped]
            .into_iter()
            .find(|state| state.as_str() == value)
            .ok_or_else(|| AppError::Database(format!("unknown audit state {value}")))
    }
}

/// The row an audit writes: one lesson draft audited by one Verifier model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunKey {
    pub lesson_id: String,
    pub draft_sha256: String,
    pub verifier_model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRun {
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
pub struct CourseAuditUsage {
    pub audits: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone)]
pub struct SqliteAuditRuns {
    pool: Database,
}

impl SqliteAuditRuns {
    pub fn new(pool: impl DatabaseHandle) -> Self {
        let pool = pool.database();
        Self { pool }
    }

    /// The lesson's most recently started audit.
    pub async fn latest(&self, lesson_id: &str) -> Result<Option<AuditRun>> {
        repository::latest_run(&self.pool, lesson_id).await
    }

    pub async fn course_usage(&self, program_id: &str) -> Result<CourseAuditUsage> {
        repository::course_usage(&self.pool, program_id).await
    }
}

#[async_trait]
impl AuditRunStore for SqliteAuditRuns {
    async fn begin(&self, key: &RunKey, policy: &str, at: i64) -> Result<AuditRun> {
        repository::open_run(&self.pool, key, policy, at).await
    }

    async fn record(&self, key: &RunKey, batch: &BatchRecord) -> Result<()> {
        repository::append_batch(&self.pool, key, batch).await
    }

    async fn finish(&self, key: &RunKey, state: RunState, at: i64) -> Result<()> {
        repository::finish_run(&self.pool, key, state, at).await
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
