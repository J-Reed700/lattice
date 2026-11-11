//! A finished session as the debrief reads it, in the engine's words.

use crate::practice::practical_dto::LearningSimulationTurnDto;
use lattice_core::domain::learning::practice::work::{
    CheckReport, CheckScope, Exercise, Form, LearnerCaseVerdict, Piece, Rung, Setting, Stage,
};
use serde_json::Value;

/// One check the session made, of the piece it was made on.
#[derive(Debug, Clone)]
pub struct SessionCheck {
    pub id: String,
    pub scope: CheckScope,
    pub piece: Piece,
    pub report: CheckReport,
    pub at: i64,
}

/// A case the learner wrote. The kind owns `body`.
#[derive(Debug, Clone)]
pub struct SessionLearnerCase {
    pub ordinal: u32,
    pub body: Value,
    pub verdict: LearnerCaseVerdict,
}

#[derive(Debug, Clone)]
pub struct SessionHint {
    pub ordinal: u32,
    pub rung: Rung,
    pub text: String,
    pub stage: Stage,
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

impl DebriefSession {
    pub fn counted(&self) -> Counted<'_> {
        Counted {
            form: self.form,
            settings: &self.settings,
            scopes: self.checks.iter().map(|check| check.scope).collect(),
            hints: &self.hints,
            events: &self.events,
        }
    }
}
