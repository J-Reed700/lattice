//! What the coding tables hold, as the session reads and writes it. The repository fills these in; the session turns
//! them into what the screen sees.

use super::problem::Problem;
use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::{
    CaseCheckDetailDto, LearningPracticalRunStatus, LearningSimulationTurnDto,
};
use crate::practice::kinds::program::dto::{CodingRunScope, ExampleVerdict};
use crate::practice::session::policy::Cue;
use lattice_core::domain::learning::drills::PatternRating;
use lattice_core::domain::learning::practice::work::{Form, KindId, Rung, Setting, Stage};
use lattice_core::domain::learning::practice::Scorecard;
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};

/// The request a write answers, so the same operation id replays the first answer instead of repeating the work.
#[derive(Debug, Clone, Copy)]
pub struct CodingOp<'a> {
    pub operation_id: &'a str,
    /// The command: `stage`, `example`, `hint`, `turn`, `setting`, `reveal`, `finish`.
    pub kind: &'static str,
    pub payload_hash: &'a str,
}

/// What a write did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied<T> {
    Done(T),
    /// The operation id was seen before, or the session was already finished: nothing was written.
    Replayed,
}

/// One row of `learning_exercise_sessions`.
#[derive(Debug, Clone)]
pub struct CodingRow {
    pub id: String,
    pub program_id: String,
    pub activity_id: String,
    pub kind: KindId,
    pub form: Form,
    pub setting: Setting,
    pub stage: Stage,
    pub budget_seconds: Option<u32>,
    pub active_ms: u64,
    pub opened_at: Option<i64>,
    pub draft: Vec<LearningLabFile>,
    pub pending_cue: Option<Cue>,
    pub submitted_at: Option<i64>,
    pub solution_seen_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub revision: i64,
    pub created_at: i64,
}

/// One row of `learning_exercise_session_events`.
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub ordinal: u32,
    pub kind: String,
    pub payload: serde_json::Value,
    pub at: i64,
}

/// One row of `learning_coding_examples`.
#[derive(Debug, Clone)]
pub struct StoredExample {
    pub ordinal: u32,
    pub args: Vec<String>,
    pub expected: String,
    pub verdict: ExampleVerdict,
    pub reference_actual: Option<String>,
    pub why_invalid: Option<String>,
    /// The ids of the misreadings the example catches.
    pub catches: Vec<String>,
}

/// The payload of a `hint` event, as it is stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredHint {
    pub ordinal: u32,
    pub rung: Rung,
    pub text: String,
    pub reason: String,
    /// Rung Point: 1-based lines of `path` in the draft the hint was written for.
    pub lines: Vec<u32>,
    pub path: Option<String>,
    pub stage: Stage,
    pub run_id: Option<String>,
    pub in_role: bool,
    pub at: i64,
}

/// A technique the session was rated on, as `ratings_json` stores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRating {
    pub pattern_id: String,
    pub name: String,
    pub family: String,
    pub cue: String,
    pub rating: PatternRating,
    pub due_at: i64,
}

/// A run of the session and the full result of each of its cases.
#[derive(Debug, Clone)]
pub struct StoredRun {
    pub run_id: String,
    pub scope: CodingRunScope,
    pub status: LearningPracticalRunStatus,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: Option<i64>,
    pub files: Vec<LearningLabFile>,
    pub created_at: i64,
    pub cases: Vec<CaseCheckDetailDto>,
}

/// The debrief row.
#[derive(Debug, Clone)]
pub struct StoredDebrief {
    pub approach: String,
    pub complexity: String,
    pub ratings: Vec<StoredRating>,
    pub model: String,
}

/// Everything stored about one session, read in one transaction.
#[derive(Debug, Clone)]
pub struct SessionBundle {
    pub row: CodingRow,
    pub lesson_id: String,
    pub title: String,
    pub activity_revision: i64,
    pub problem: Problem,
    pub events: Vec<StoredEvent>,
    pub examples: Vec<StoredExample>,
    pub runs: Vec<StoredRun>,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub scorecard: Option<Scorecard>,
    pub debrief: Option<StoredDebrief>,
}

/// What a new session needs besides the request's operation.
#[derive(Clone)]
pub struct NewCodingSession<'a> {
    pub program_id: &'a str,
    pub activity_id: &'a str,
    pub setting: Setting,
    /// The learner's choice. Ignored in Coached, which has no budget.
    pub budget_seconds: Option<u32>,
    /// The budget Mock and Watched take when the learner chose none, of the problem the activity holds.
    pub default_budget: &'a (dyn Fn(&Problem) -> Result<Option<u32>> + Send + Sync),
}

/// A stage move that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageMove {
    pub from: Stage,
    pub to: Stage,
    /// The stages strictly between the two, when moving forward.
    pub skipped: Vec<Stage>,
}

/// A hint the writer produced, to be stored.
#[derive(Debug, Clone)]
pub struct NewHint<'a> {
    pub rung: Rung,
    pub text: &'a str,
    pub reason: &'a str,
    pub lines: &'a [u32],
    pub path: Option<&'a str>,
    /// Watched: also added to the transcript as the interviewer's turn.
    pub in_role: bool,
    pub model: &'a str,
}

/// A run that finished, as the session records it.
#[derive(Debug, Clone)]
pub struct RunFinished<'a> {
    pub run_id: &'a str,
    pub scope: CodingRunScope,
    pub passed: u32,
    pub total: u32,
    /// An input run in Coached: what the reference returns.
    pub reference_actual: Option<&'a str>,
    /// The run is the learner's submit: every case, and the runtime gave an answer.
    pub submit: bool,
}

/// What finishing a session writes besides the session's own row.
#[derive(Debug, Clone)]
pub struct FinishWrite<'a> {
    pub scorecard: &'a Scorecard,
    pub approach: &'a str,
    pub complexity: &'a str,
    pub model: &'a str,
    pub language: &'a str,
    pub pattern_ids: &'a [String],
    pub rating: PatternRating,
    pub files: &'a [LearningLabFile],
    /// Cases passed and run on the last submit, for the course's evidence.
    pub passed: u32,
    pub total: u32,
    /// The most help any setting of the session gave: `full_help` or `hints_only`.
    pub help: &'static str,
    pub hints: u32,
}
