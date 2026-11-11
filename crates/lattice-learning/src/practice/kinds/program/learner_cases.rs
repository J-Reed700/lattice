//! The cases a learner writes for a program: the arguments of a call and what it should return.

use crate::practice::session::kind::{Citation, LearnerCases};
use crate::practice::session::record::SessionLearnerCase;
use lattice_core::domain::learning::practice::work::Brief;
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::json;

#[cfg(test)]
mod tests;

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
}
