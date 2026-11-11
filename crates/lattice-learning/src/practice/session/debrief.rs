//! The debrief of a session, for every kind of work: facts counted from what happened, and a scorecard the model
//! writes and the app checks against the session's own data.

use crate::practice::session::kind::Kind;
use crate::practice::session::record::{Counted, DebriefSession};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{Form, PartRef, Rung, Setting, Stage};
use lattice_core::domain::learning::practice::ScoreStatus;
use lattice_core::shared::error::Result;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// What each criterion can earn.
pub const MAX_POINTS: u8 = 4;

/// One fact behind a row, checked against the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    /// Words copied from a learner turn.
    Quote {
        turn_id: String,
        text: String,
    },
    /// Parts of the piece a check was made on.
    Part {
        check_id: String,
        parts: Vec<PartRef>,
    },
    Check {
        check_id: String,
    },
    LearnerCase {
        ordinal: u32,
    },
    Hint {
        ordinal: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebriefRow {
    pub criterion_id: String,
    pub title: String,
    pub status: ScoreStatus,
    pub points: Option<u8>,
    pub max_points: u8,
    pub evidence: Vec<Evidence>,
    pub feedback: String,
    pub next_step: String,
}

/// What the model writes: a row for each criterion, and what the kind has it state beside them, by key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Debrief {
    pub rows: Vec<DebriefRow>,
    pub stated: Vec<(String, String)>,
}

/// What the session shows beside the scores, counted rather than judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub stage_ms: Vec<(Stage, u64)>,
    pub checks: u32,
    /// The checks of every case.
    pub submits: u32,
    pub hints: Vec<(Rung, u32)>,
    pub settings: Vec<Setting>,
}

pub fn facts(_kind: &Kind, _counted: &Counted<'_>) -> Facts {
    Facts {
        stage_ms: Vec::new(),
        checks: 0,
        submits: 0,
        hints: Vec::new(),
        settings: Vec::new(),
    }
}

pub fn system(_kind: &Kind, _form: Form) -> String {
    String::new()
}

pub fn schema(_kind: &Kind, _form: Form) -> Value {
    Value::Null
}

pub fn prompt(_kind: &Kind, _session: &DebriefSession) -> Result<String> {
    Ok(String::new())
}

pub async fn debrief(
    _llm: &dyn LLMPort,
    _kind: &Kind,
    _session: &DebriefSession,
) -> Result<Debrief> {
    Ok(Debrief {
        rows: Vec::new(),
        stated: Vec::new(),
    })
}
