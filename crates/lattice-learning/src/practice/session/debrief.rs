//! The debrief of a session, for every kind of work: facts counted from what happened, and a scorecard the model
//! writes and the app checks against the session's own data.

use crate::lessons::structured_call::reask;
use crate::model_call;
use crate::practice::session::kind::Kind;
use crate::practice::session::record::DebriefSession;
use answer::{ask, examine, settle};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{Form, PartRef, Rung, Setting, Stage};
use lattice_core::domain::learning::practice::ScoreStatus;
use lattice_core::shared::error::Result;
use serde_json::Value;

mod answer;
mod plan;
mod tally;
mod telling;

pub use tally::facts;

use plan::Plan;
use telling::told;

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

pub fn system(kind: &Kind, form: Form) -> String {
    Plan::new(kind, form).system()
}

pub fn schema(kind: &Kind, form: Form) -> Value {
    Plan::new(kind, form).schema()
}

pub fn prompt(kind: &Kind, session: &DebriefSession) -> Result<String> {
    told(&Plan::new(kind, session.form), session)
}

/// Asks for the debrief, checks every item of evidence, and asks once more with every fault; evidence that is still
/// wrong after that is dropped, and a row left without any becomes insufficient evidence.
pub async fn debrief(llm: &dyn LLMPort, kind: &Kind, session: &DebriefSession) -> Result<Debrief> {
    let plan = Plan::new(kind, session.form);
    let request = model_call::structured(
        &plan.system(),
        told(&plan, session)?,
        plan.schema(),
        0,
        "low",
    );
    let (raw, answer) = ask(llm, &plan, request.clone()).await?;
    let examined = examine(&plan, session, &answer)?;
    if examined.faults.is_empty() {
        return settle(&plan, answer, examined);
    }
    let reason = format!(
        "The app checked the evidence against the session and found these faults:
{}",
        examined.faults.join(
            "
"
        )
    );
    let (_, answer) = ask(llm, &plan, reask(request, &raw, &reason)).await?;
    let examined = examine(&plan, session, &answer)?;
    settle(&plan, answer, examined)
}
