//! The debrief of a session, for every kind of work: facts counted from what happened, and a scorecard the model
//! writes and the app checks against the session's own data.

use crate::practice::session::kind::Kind;
use crate::practice::session::record::DebriefSession;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{PartRef, Rung, Setting, Stage};
use lattice_core::domain::learning::practice::ScoreStatus;
use lattice_core::shared::error::Result;

mod answer;
mod plan;
mod statements;
mod tally;
mod telling;
mod things;

pub use tally::facts;
pub use telling::Names;
#[cfg(test)]
pub(crate) use {answer::DebriefAnswer, statements::DebriefStatements};

use plan::Plan;
use telling::render;

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

/// Asks for the scorecard rows and for what the kind has the debrief state beside them.
pub async fn debrief(llm: &dyn LLMPort, kind: &Kind, session: &DebriefSession) -> Result<Debrief> {
    let plan = Plan::new(kind, session.form);
    let rendered = render(kind, session)?;
    let (rows, stated) = tokio::try_join!(
        answer::rows(llm, &plan, &rendered),
        statements::stated(llm, &plan, session)
    )?;
    Ok(Debrief { rows, stated })
}
