//! The cases a run covers, by scope.
use crate::practice::coding::dto::CodingRunScope;
use crate::practice::coding::dto::ExampleVerdict;
use crate::practice::coding::problem::{CaseVisibility, Problem, ProblemCase};
use crate::practice::coding::records::StoredExample;
use lattice_core::shared::error::{AppError, Result};

/// The id of the one case an input run holds.
pub const INPUT_CASE_ID: &str = "input";
/// What a learner example's case id starts with; the ordinal follows.
pub const MINE_CASE_PREFIX: &str = "mine-";

/// The cases of one run, and which of them are the learner's own examples.
pub struct RunCases {
    pub cases: Vec<ProblemCase>,
    pub mine_ids: Vec<String>,
}

fn learner_case(
    problem: &Problem,
    id: String,
    category: String,
    args: Vec<String>,
    expected: Option<String>,
) -> ProblemCase {
    ProblemCase {
        id,
        category,
        args,
        expected,
        compare: problem.default_compare(),
        visibility: CaseVisibility::Example,
        scale: false,
        digest: None,
    }
}

/// Examples: the statement's plus the learner's right ones. All: every case. Input: one case with no expected value.
pub fn cases_for(
    problem: &Problem,
    examples: &[StoredExample],
    scope: CodingRunScope,
    input: Option<&[String]>,
) -> Result<RunCases> {
    match scope {
        CodingRunScope::All => Ok(RunCases {
            cases: problem.cases.clone(),
            mine_ids: Vec::new(),
        }),
        CodingRunScope::Examples => {
            let mut cases = problem.examples().cloned().collect::<Vec<_>>();
            let mut mine_ids = Vec::new();
            for example in examples
                .iter()
                .filter(|example| example.verdict == ExampleVerdict::Right)
            {
                let id = format!("{MINE_CASE_PREFIX}{}", example.ordinal);
                cases.push(learner_case(
                    problem,
                    id.clone(),
                    format!("Your example {}", example.ordinal),
                    example.args.clone(),
                    Some(example.expected.clone()),
                ));
                mine_ids.push(id);
            }
            Ok(RunCases { cases, mine_ids })
        }
        CodingRunScope::Input => {
            let args = input
                .ok_or_else(|| AppError::InvalidInput("An input run needs an input.".into()))?;
            if args.len() != problem.spec.signature.params.len() {
                return Err(AppError::InvalidInput(
                    "The input needs one value for each parameter.".into(),
                ));
            }
            Ok(RunCases {
                cases: vec![learner_case(
                    problem,
                    INPUT_CASE_ID.into(),
                    "Your input".into(),
                    args.to_vec(),
                    None,
                )],
                mine_ids: Vec::new(),
            })
        }
    }
}
