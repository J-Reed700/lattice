//! The cases a learner writes for a program: the arguments of a call and what it should return.

use crate::practice::session::kind::LearnerCases;
use lattice_core::domain::learning::practice::work::Brief;
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;

#[cfg(test)]
mod tests;

pub struct ProgramLearnerCases;

#[derive(Deserialize)]
struct LearnerCaseBody {
    args: Vec<String>,
    expected: String,
}

impl LearnerCases for ProgramLearnerCases {
    fn show(&self, _brief: &Brief, body: &serde_json::Value) -> Result<String> {
        let body: LearnerCaseBody = serde_json::from_value(body.clone()).map_err(|error| {
            AppError::InvalidState(format!(
                "The case is not a program's learner case: {error}."
            ))
        })?;
        Ok(format!(
            "({}) expects {}",
            body.args.join(", "),
            body.expected
        ))
    }
}
