//! The cases a learner writes for a program: the arguments of a call and what it should return.

use crate::practice::coding::problem::{
    CaseCompare, CaseVisibility as ProblemCaseVisibility, ProblemCase,
};
use crate::practice::kinds::program::shape::{case_of, problem_case_of};
use crate::practice::session::kind::{Citation, LearnerCases};
use crate::practice::session::record::SessionLearnerCase;
use lattice_core::domain::learning::practice::work::{Brief, Case, CaseVisibility, Exercise};
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::json;

#[cfg(test)]
mod tests;

/// What the id of a case the learner wrote starts with; the ordinal follows.
pub const MINE_CASE_PREFIX: &str = "mine-";

pub struct ProgramLearnerCases;

#[derive(Deserialize)]
struct LearnerCaseBody {
    args: Vec<String>,
    expected: String,
}

fn read_body(body: &serde_json::Value) -> Result<LearnerCaseBody> {
    serde_json::from_value(body.clone()).map_err(|error| {
        AppError::InvalidState(format!(
            "The case is not a program's learner case: {error}."
        ))
    })
}

impl LearnerCases for ProgramLearnerCases {
    fn show(&self, _brief: &Brief, body: &serde_json::Value) -> Result<String> {
        let body = read_body(body)?;
        Ok(format!(
            "({}) expects {}",
            body.args.join(", "),
            body.expected
        ))
    }

    fn key(&self) -> String {
        "examples".to_owned()
    }

    fn account(&self, _brief: &Brief, case: &SessionLearnerCase) -> Result<serde_json::Value> {
        let body = read_body(&case.body)?;
        Ok(json!({
            "ordinal": case.ordinal,
            "args": body.args,
            "expected": body.expected,
            "verdict": case.verdict.label().to_lowercase(),
        }))
    }

    fn cited(&self) -> Citation {
        Citation {
            tag: "example".to_owned(),
            sentence: "An example: ref is the example's ordinal.".to_owned(),
        }
    }

    fn case(&self, exercise: &Exercise, case: &SessionLearnerCase) -> Result<Case> {
        let body = read_body(&case.body)?;
        let compare = match exercise
            .cases
            .iter()
            .find(|case| case.visibility == CaseVisibility::Shown)
        {
            Some(shown) => problem_case_of(shown)?.compare,
            None => CaseCompare::Exact,
        };
        case_of(&ProblemCase {
            id: format!("{MINE_CASE_PREFIX}{}", case.ordinal),
            category: format!("Your example {}", case.ordinal),
            args: body.args,
            expected: Some(body.expected),
            compare,
            visibility: ProblemCaseVisibility::Example,
            scale: false,
            digest: None,
        })
    }
}
