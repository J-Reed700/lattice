//! The one case an input run holds.
use crate::practice::coding::problem::{CaseVisibility, ProblemCase};
use crate::practice::kinds::program::shape::{case_of, default_compare_of, spec_of};
use lattice_core::domain::learning::practice::work::{Case, Exercise};
use lattice_core::shared::error::{AppError, Result};

/// The id of the one case an input run holds.
pub const INPUT_CASE_ID: &str = "input";

/// The case that runs `input` and asks for no verdict.
pub fn input_case(exercise: &Exercise, args: &[String]) -> Result<Case> {
    if args.len() != spec_of(&exercise.brief)?.signature.params.len() {
        return Err(AppError::InvalidInput(
            "The input needs one value for each parameter.".into(),
        ));
    }
    case_of(&ProblemCase {
        id: INPUT_CASE_ID.into(),
        category: "Your input".into(),
        args: args.to_vec(),
        expected: None,
        compare: default_compare_of(exercise)?,
        visibility: CaseVisibility::Example,
        scale: false,
        digest: None,
    })
}
