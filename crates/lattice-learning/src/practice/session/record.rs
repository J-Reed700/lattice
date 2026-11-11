//! A finished session as the debrief reads it, in the engine's words.

use crate::practice::practical_dto::LearningSimulationTurnDto;
use crate::practice::session::debrief::Debrief;
use crate::practice::session::policy::Cue;
use lattice_core::domain::learning::drills::PatternRating;
use lattice_core::domain::learning::practice::work::{
    Case, CheckReport, CheckScope, CheckStatus, Exercise, Form, LearnerCaseVerdict, PartRef, Piece,
    Rung, Setting, Stage,
};
use lattice_core::domain::learning::practice::Scorecard;
use lattice_core::shared::error::Result;
use serde_json::Value;

/// A case a check ran, and whether the learner wrote it.
#[derive(Debug, Clone)]
pub struct CheckedCase {
    pub case: Case,
    pub own: bool,
}

/// One check the session made, of the piece it was made on.
#[derive(Debug, Clone)]
pub struct SessionCheck {
    pub id: String,
    pub scope: CheckScope,
    pub piece: Piece,
    pub report: CheckReport,
    pub at: i64,
    pub status: CheckStatus,
    /// The cases the check ran, in the order of the report's results.
    pub cases: Vec<CheckedCase>,
}

/// A case the learner wrote. The kind owns `body`.
#[derive(Debug, Clone)]
pub struct SessionLearnerCase {
    pub ordinal: u32,
    pub body: Value,
    pub verdict: LearnerCaseVerdict,
    /// The ids of the misreadings it catches.
    pub catches: Vec<String>,
    /// What the kind's check of the case found beyond its verdict. The kind owns it.
    pub found: Value,
}

#[derive(Debug, Clone)]
pub struct SessionHint {
    pub ordinal: u32,
    pub rung: Rung,
    pub text: String,
    pub stage: Stage,
    /// The last check made when the hint was given.
    pub check_id: Option<String>,
    pub reason: String,
    /// The parts of the piece the hint points at.
    pub parts: Vec<PartRef>,
    /// The counterpart gave it in role.
    pub in_role: bool,
    pub at: i64,
}

/// What a row of the session's event log says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// The clock started.
    Opened,
    /// The clock stopped.
    Closed,
    /// The learner moved to a stage, when the row names one.
    Moved(Option<Stage>),
    Other,
}

#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub ordinal: u32,
    pub kind: EventKind,
    pub at: i64,
}

/// Everything the debrief reads about one session.
#[derive(Debug, Clone)]
pub struct DebriefSession {
    pub exercise: Exercise,
    pub form: Form,
    pub settings: Vec<Setting>,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub checks: Vec<SessionCheck>,
    pub learner_cases: Vec<SessionLearnerCase>,
    pub hints: Vec<SessionHint>,
    pub events: Vec<SessionEvent>,
    /// The learner's work as it stood at the end.
    pub draft: Piece,
}

/// What the facts of a session are counted from.
pub struct Counted<'a> {
    pub form: Form,
    pub settings: &'a [Setting],
    pub scopes: Vec<CheckScope>,
    pub hints: &'a [SessionHint],
    pub events: &'a [SessionEvent],
}

fn counted<'a>(
    form: Form,
    settings: &'a [Setting],
    checks: &[SessionCheck],
    hints: &'a [SessionHint],
    events: &'a [SessionEvent],
) -> Counted<'a> {
    Counted {
        form,
        settings,
        scopes: checks.iter().map(|check| check.scope).collect(),
        hints,
        events,
    }
}

impl DebriefSession {
    pub fn counted(&self) -> Counted<'_> {
        counted(
            self.form,
            &self.settings,
            &self.checks,
            &self.hints,
            &self.events,
        )
    }
}

/// The request a write answers, so the same operation id replays the first answer instead of repeating the work.
#[derive(Debug, Clone, Copy)]
pub struct Op<'a> {
    pub operation_id: &'a str,
    /// The command the operation id was first used for.
    pub command: &'static str,
    pub payload_hash: &'a str,
}

/// What a write did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied<T> {
    Done(T),
    /// The operation id was seen before, or the session was already finished: nothing was written.
    Replayed,
}

/// A stage move that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageMove {
    pub from: Stage,
    pub to: Stage,
    /// The stages strictly between the two, when moving forward.
    pub skipped: Vec<Stage>,
}

/// A hint to be stored.
#[derive(Debug, Clone)]
pub struct NewHint<'a> {
    pub rung: Rung,
    pub text: &'a str,
    pub reason: &'a str,
    pub parts: &'a [PartRef],
    /// The counterpart gives it in role: it is also a turn of the transcript.
    pub in_role: bool,
    pub model: &'a str,
}

/// The most help any setting of a session gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Help {
    Full,
    HintsOnly,
}

/// How a session went, for the schedules of what it practised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub all_passed: bool,
    pub highest_rung: Option<Rung>,
    pub solution_seen_before_passing: bool,
}

/// What finishing a session writes besides the session's own row.
pub struct Finish<'a> {
    pub debrief: &'a Debrief,
    pub model: &'a str,
    pub rating: PatternRating,
    /// The learner's work as it stood at the end.
    pub draft: &'a Piece,
    /// Cases passed and run on the last submit.
    pub passed: u32,
    pub total: u32,
    pub help: Help,
    pub hints: u32,
}

/// What a new session needs besides the request's operation.
pub struct NewSession<'a> {
    pub program_id: &'a str,
    pub activity_id: &'a str,
    pub setting: Setting,
    /// The learner's choice. Ignored in Coached, which has no budget.
    pub budget_seconds: Option<u32>,
    /// The budget a timed setting takes when the learner chose none, of the exercise the store reads.
    pub default_budget: &'a (dyn Fn(&Exercise) -> Result<Option<u32>> + Send + Sync),
}

/// The session's own row.
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub program_id: String,
    pub activity_id: String,
    pub form: Form,
    pub setting: Setting,
    pub stage: Stage,
    pub budget_seconds: Option<u32>,
    pub active_ms: u64,
    pub opened_at: Option<i64>,
    pub draft: Piece,
    pub pending_cue: Option<Cue>,
    pub submitted_at: Option<i64>,
    pub solution_seen_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub revision: i64,
    pub created_at: i64,
}

/// Everything stored about one session, read in one transaction.
#[derive(Debug, Clone)]
pub struct Session {
    pub row: SessionRow,
    pub activity_revision: i64,
    pub exercise: Exercise,
    /// The settings the session has used, in order, from the one it started in.
    pub settings: Vec<Setting>,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub checks: Vec<SessionCheck>,
    pub learner_cases: Vec<SessionLearnerCase>,
    pub hints: Vec<SessionHint>,
    pub events: Vec<SessionEvent>,
    /// The draft the counterpart last saw.
    pub draft_seen: Option<Piece>,
    pub lesson_id: String,
    /// The debrief, once the session is finished and has one.
    pub debrief: Option<SessionDebrief>,
}

/// A technique the session's rating was recorded against, and when it is due again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechniqueRating {
    pub id: String,
    pub name: String,
    pub family: String,
    pub cue: String,
    pub rating: PatternRating,
    pub due_at: i64,
}

/// The stored debrief of a finished session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDebrief {
    pub scorecard: Scorecard,
    /// What the kind had the model state beside the scores, by key.
    pub stated: Vec<(String, String)>,
    pub model: String,
    pub techniques: Vec<TechniqueRating>,
}

impl Session {
    /// What the facts of the session are counted from.
    pub fn counted(&self) -> Counted<'_> {
        counted(
            self.row.form,
            &self.settings,
            &self.checks,
            &self.hints,
            &self.events,
        )
    }

    /// The whole session for the debrief: nothing is cut by a setting.
    pub fn debrief(&self, draft: Piece) -> DebriefSession {
        DebriefSession {
            exercise: self.exercise.clone(),
            form: self.row.form,
            settings: self.settings.clone(),
            turns: self.turns.clone(),
            checks: self.checks.clone(),
            learner_cases: self.learner_cases.clone(),
            hints: self.hints.clone(),
            events: self.events.clone(),
            draft,
        }
    }
}
